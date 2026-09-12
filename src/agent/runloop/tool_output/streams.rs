//! Tool output rendering with token-aware truncation
//!
//! This module handles formatting and displaying tool output to the user.
//! It uses a **token-based truncation strategy** instead of naive line limits,
//! which aligns with how LLMs consume context.
//!
//! ## Truncation Strategy
//!
//! Instead of hard line limits (e.g., "show first 128 + last 128 lines"), we use:
//! - **Token budget**: 25,000 tokens max per tool response
//! - **Head+Tail preservation**: Keep first ~50% and last ~50% of tokens
//! - **Token-aware**: Uses heuristic approximation for token counting
//!   (1 token ≈ 3.5 chars for regular content)
//!
//! ### Why Token-Based?
//!
//! 1. **Aligns with reality**: Tokens matter for context window, not lines
//!    - 256 short lines (~1-2k tokens) < 100 long lines (~10k tokens)
//!
//! 2. **Better for incomplete outputs**: Long build logs or test results often have
//!    critical info at the end (errors, summaries). Head+tail preserves both.
//!
//! 3. **Fewer tool calls needed**: Model can absorb more meaningful information
//!    per call instead of making multiple sequential calls to work around limits.
//!
//! 4. **Consistent across tools**: All tool outputs use the same token budget,
//!    not arbitrary per-tool line limits.
//!
//! ### UI Display Limits (Separate Layer)
//!
//! The token limit applies to what we *send to the model*. Display rendering has
//! separate safeguards to prevent UI lag:
//! - `MAX_LINE_LENGTH: 150`: Prevents extremely long lines from hanging the TUI
//! - `INLINE_STREAM_MAX_LINES: 30`: Limits visible output in inline mode
//! - `MAX_CODE_LINES: 30`: For code fence blocks (full output in spool files)
//!
//! Full output is spooled to workspace `.vtcode/tool-output/` for later review.
//! For very large outputs, files are saved to the user cache's
//! `large-output/<session_hash>/call_<id>.output`
//! with a notification displayed to the client.

use std::borrow::Cow;

use anstyle::{AnsiColor, Effects, Reset, Style as AnsiStyle};
use anyhow::Result;
use smallvec::SmallVec;
use vtcode_commons::diff_preview::{
    DiffDisplayKind, DiffDisplayLine, diff_display_line_number_width, display_lines_from_unified_diff,
};
use vtcode_commons::preview::{
    display_width, excerpt_text_lines, format_hidden_lines_summary as shared_hidden_lines_summary,
    truncate_with_ellipsis,
};
use vtcode_core::config::ToolOutputMode;
use vtcode_core::config::loader::VTCodeConfig;
use vtcode_core::tools::tool_intent;
use vtcode_core::utils::ansi::{AnsiRenderer, MessageStyle};

use super::files::colorize_diff_summary_line;
use super::styles::{GitStyles, LsStyles, select_line_style};
#[path = "streams_helpers.rs"]
mod streams_helpers;
pub(crate) use streams_helpers::{
    build_markdown_code_block, render_code_fence_blocks, resolve_stdout_tail_limit, strip_ansi_codes,
};
use streams_helpers::{
    looks_like_diff_content, select_stream_lines_streaming, should_render_as_code_block, spool_output_if_needed,
};

/// Maximum number of lines to display in inline mode before truncating
const INLINE_STREAM_MAX_LINES: usize = 30;
/// Number of head lines to show for run-command output previews
const RUN_COMMAND_HEAD_PREVIEW_LINES: usize = 3;
/// Number of tail lines to show for run-command output previews
const RUN_COMMAND_TAIL_PREVIEW_LINES: usize = 3;
/// Maximum line length before truncation to prevent TUI hang
const MAX_LINE_LENGTH: usize = 150;
/// Size threshold (bytes) below which output is displayed inline vs. spooled
const DEFAULT_SPOOL_THRESHOLD: usize = 50_000; // 50KB — UI render truncation
/// Maximum number of lines to display in code fence blocks before truncating.
/// Kept low to prevent TUI flooding — full output is in spool files.
const MAX_CODE_LINES: usize = 30;
/// Size threshold (bytes) at which to skip preview entirely
const EXTREME_OUTPUT_THRESHOLD_MB: usize = 2_000_000;
/// Size threshold (bytes) for using new large output handler with hashed directories
const LARGE_OUTPUT_NOTIFICATION_THRESHOLD: usize = 50_000; // 50KB — triggers spool-to-file for UI

enum HiddenLinesNoticeKind {
    CommandPreview,
    Generic,
    TokenBudget,
}

fn hidden_lines_notice(hidden: usize, kind: HiddenLinesNoticeKind) -> String {
    match kind {
        HiddenLinesNoticeKind::CommandPreview => {
            format!("    {} (/share html for full transcript)", shared_hidden_lines_summary(hidden))
        }
        HiddenLinesNoticeKind::Generic => {
            format!("[... {} line{} truncated ...]", hidden, if hidden == 1 { "" } else { "s" })
        }
        HiddenLinesNoticeKind::TokenBudget => "[... content truncated by token budget ...]".to_string(),
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "Intentional compatibility, platform, or test-only suppression."
)]
fn render_preview_line(
    renderer: &mut AnsiRenderer,
    display_line: &str,
    rendered_line: Option<&str>,
    prefix: Option<&str>,
    truncate_line: bool,
    fallback_style: MessageStyle,
    override_style: Option<AnsiStyle>,
) -> Result<()> {
    if display_line.is_empty() {
        return Ok(());
    }

    let line = rendered_line.unwrap_or(display_line);

    if !truncate_line || display_width(line) <= MAX_LINE_LENGTH {
        return match prefix {
            Some(pfx) => {
                let mut buf = String::with_capacity(pfx.len() + line.len());
                buf.push_str(pfx);
                buf.push_str(line);
                renderer.line_with_override_style(
                    fallback_style,
                    override_style.unwrap_or(fallback_style.style()),
                    &buf,
                )
            }
            None => renderer.line_with_override_style(
                fallback_style,
                override_style.unwrap_or(fallback_style.style()),
                line,
            ),
        };
    }

    let truncated = truncate_with_ellipsis(line, MAX_LINE_LENGTH, "...");
    let text = if let Some(pfx) = prefix {
        let mut buf = String::with_capacity(pfx.len() + truncated.len());
        buf.push_str(pfx);
        buf.push_str(&truncated);
        buf
    } else {
        truncated
    };

    renderer.line_with_override_style(fallback_style, override_style.unwrap_or(fallback_style.style()), &text)
}

fn highlight_diff_content(
    content: &str,
    bg: Option<anstyle::Color>,
    word_ranges: &[(usize, usize)],
    word_bg: Option<anstyle::Color>,
) -> Option<String> {
    // Add/del bodies stay solid on the line tint (default fg). Syntax tokens
    // leak brightness onto the band; only word chips use a stronger bg.
    if content.is_empty() {
        return None;
    }
    let bg = bg?;
    let mut out = String::with_capacity(content.len() + 16);
    // Reset first so no prior SGR state bleeds into this run.
    out.push_str(&Reset.to_string());

    if word_ranges.is_empty() || word_bg.is_none() {
        out.push_str(&AnsiStyle::new().bg_color(Some(bg)).render().to_string());
        out.push_str(content);
        out.push_str(&Reset.to_string());
        return Some(out);
    }

    // Two-level: line tint on unchanged spans; stronger chip on changed.
    let word_bg = word_bg.expect("checked above");
    let mut cursor = 0usize;
    for &(start, end) in word_ranges {
        let start = start.min(content.len());
        let end = end.min(content.len()).max(start);
        if start > cursor {
            out.push_str(&AnsiStyle::new().bg_color(Some(bg)).render().to_string());
            out.push_str(&content[cursor..start]);
            out.push_str(&Reset.to_string());
        }
        if end > start {
            out.push_str(&AnsiStyle::new().bg_color(Some(word_bg)).render().to_string());
            out.push_str(&content[start..end]);
            out.push_str(&Reset.to_string());
        }
        cursor = end.max(cursor);
    }
    if cursor < content.len() {
        out.push_str(&AnsiStyle::new().bg_color(Some(bg)).render().to_string());
        out.push_str(&content[cursor..]);
        out.push_str(&Reset.to_string());
    }
    Some(out)
}

fn format_diff_line_with_gutter_and_syntax<'a>(
    line: &DiffDisplayLine,
    base_style: Option<AnsiStyle>,
    line_number_width: usize,
    word_bg: Option<anstyle::Color>,
    out: &'a mut String,
) -> &'a str {
    use std::fmt::Write as _;

    out.clear();
    let (marker, mut content) = match line.kind {
        DiffDisplayKind::Addition => ('+', line.text.as_str()),
        DiffDisplayKind::Deletion => ('-', line.text.as_str()),
        DiffDisplayKind::Context => (' ', line.text.as_str()),
        DiffDisplayKind::Metadata | DiffDisplayKind::HunkHeader => {
            out.push_str(&line.numbered_text(line_number_width));
            return out;
        }
    };
    if content.is_empty() {
        content = " ";
    }

    // Add/delete content is truncated to a single line at MAX_LINE_LENGTH so
    // these rows never wrap or get padded into continuation rows.
    let content_owned;
    let mut truncated = false;
    let content: &str = if !matches!(marker, ' ') {
        if display_width(content) > MAX_LINE_LENGTH {
            content_owned = truncate_with_ellipsis(content, MAX_LINE_LENGTH, "...");
            truncated = true;
            &content_owned
        } else {
            content
        }
    } else {
        content
    };

    let bg = base_style.and_then(|style| style.get_bg_color());
    // Unified gutter: bright red/green only on the sign; numbers stay dim
    // grey on the same full-width tint. No BOLD/DIM bleed into the body.
    let marker_style = match marker {
        '+' => AnsiStyle::new()
            .fg_color(Some(anstyle::Color::Ansi(AnsiColor::BrightGreen)))
            .bg_color(bg),
        '-' => AnsiStyle::new()
            .fg_color(Some(anstyle::Color::Ansi(AnsiColor::BrightRed)))
            .bg_color(bg),
        _ => AnsiStyle::new().fg_color(Some(anstyle::Color::Ansi(AnsiColor::BrightBlack))),
    };
    let gutter_style = AnsiStyle::new()
        .fg_color(Some(anstyle::Color::Ansi(AnsiColor::BrightBlack)))
        .bg_color(bg)
        .effects(Effects::DIMMED);
    let reset = Reset;
    out.reserve(line.text.len() + 32);
    // Single gutter: `sign + number + │ + content`. The `│` keeps markdown
    // bullets (`- foo`) distinct from the diff marker (`+`/`-`). Every span
    // carries the line tint so the band is full-width. Full Reset before
    // each span so SGR state never leaks into the next.
    let line_no = match marker {
        '+' => line.new_line,
        '-' => line.old_line,
        _ => line.new_line.or(line.old_line),
    }
    .unwrap_or_default();
    let _ = write!(out, "{reset}");
    let _ = write!(out, "{}", marker_style.render());
    out.push_str(match marker {
        '+' => "+",
        '-' => "-",
        _ => " ",
    });
    let _ = write!(out, "{reset}");
    let _ = write!(out, "{}", gutter_style.render());
    let _ = write!(out, "{line_no:>line_number_width$} │ ");
    let _ = write!(out, "{reset}");
    // Two-level body: line tint + word chips on tokens that differ from the pair.
    // Skip chips on truncated rows — `changed` offsets are into the original
    // text and would highlight the wrong slice after ellipsis truncation.
    let word_ranges: &[(usize, usize)] = if matches!(marker, '+' | '-') && !content.is_empty() && !truncated {
        &line.changed
    } else {
        &[]
    };
    if let Some(highlighted) = highlight_diff_content(content, bg, word_ranges, word_bg) {
        out.push_str(&highlighted);
    } else if base_style.is_some() {
        let body = AnsiStyle::new().bg_color(bg);
        let _ = write!(out, "{}", body.render());
        out.push_str(content);
        let _ = write!(out, "{reset}");
    } else {
        out.push_str(content);
        let _ = write!(out, "{reset}");
    }
    out
}

fn collect_run_command_preview(content: &str) -> (SmallVec<[&str; 32]>, usize, usize) {
    let preview = excerpt_text_lines(content, RUN_COMMAND_HEAD_PREVIEW_LINES, RUN_COMMAND_TAIL_PREVIEW_LINES);
    let mut collected: SmallVec<[&str; 32]> = SmallVec::with_capacity(preview.head.len() + preview.tail.len());
    collected.extend(preview.head.iter().copied());
    collected.extend(preview.tail.iter().copied());
    (collected, preview.total, preview.hidden_count)
}

async fn render_run_command_preview(
    renderer: &mut AnsiRenderer,
    content: &str,
    tool_name: Option<&str>,
    fallback_style: MessageStyle,
    disable_spool: bool,
    config: Option<&VTCodeConfig>,
) -> Result<()> {
    let run_tool_name = tool_name.unwrap_or(vtcode_core::config::constants::tools::RUN_PTY_CMD);
    if !disable_spool && let Ok(Some(log_path)) = spool_output_if_needed(content, run_tool_name, config).await {
        let total = content.lines().count();
        renderer.line(
            MessageStyle::ToolDetail,
            &format!(
                "Command output too large ({} bytes, {} lines), spooled to: {}",
                content.len(),
                total,
                log_path.display()
            ),
        )?;
    }

    let (preview_lines, _total, hidden) = collect_run_command_preview(content);
    if preview_lines.is_empty() {
        return Ok(());
    }

    // Show hidden lines notice if needed
    if hidden > 0 {
        renderer.line(MessageStyle::ToolDetail, &hidden_lines_notice(hidden, HiddenLinesNoticeKind::CommandPreview))?;
    }

    // Render command output with bash syntax highlighting.
    // Wrap the preview lines in markdown code fences with "bash" language hint
    // to get proper syntax highlighting for command output.
    let lines_vec: SmallVec<[&str; 32]> = preview_lines.iter().copied().collect();
    let markdown = build_markdown_code_block(&lines_vec, Some("bash"), true);
    renderer.render_markdown_output(fallback_style, &markdown)?;

    Ok(())
}

#[allow(
    clippy::too_many_arguments,
    reason = "Intentional compatibility, platform, or test-only suppression."
)]
pub(crate) fn render_diff_content_block(
    renderer: &mut AnsiRenderer,
    diff_content: &str,
    tool_name: Option<&str>,
    git_styles: &GitStyles,
    ls_styles: &LsStyles,
    fallback_style: MessageStyle,
    mode: ToolOutputMode,
    tail_limit: usize,
) -> Result<()> {
    let diff_lines = display_lines_from_unified_diff(diff_content);
    let total = diff_lines.len();
    let effective_limit = if renderer.prefers_untruncated_output() || matches!(mode, ToolOutputMode::Full) {
        tail_limit.max(1000)
    } else {
        tail_limit
    };
    let (lines_slice, truncated) = if total > effective_limit {
        let start = total.saturating_sub(effective_limit);
        (&diff_lines[start..], total > effective_limit)
    } else {
        (&diff_lines[..], false)
    };

    if truncated {
        let hidden = total.saturating_sub(lines_slice.len());
        if hidden > 0 {
            renderer.line(MessageStyle::ToolDetail, &hidden_lines_notice(hidden, HiddenLinesNoticeKind::Generic))?;
        }
    }

    let line_number_width = diff_display_line_number_width(lines_slice);
    let color_enabled = renderer.capabilities().supports_color();
    let mut formatted_buffer = String::with_capacity(256);
    let mut display_buffer = String::with_capacity(256);

    for line in lines_slice {
        display_buffer.clear();
        let raw_line = line.numbered_text(line_number_width);
        if raw_line.is_empty() {
            continue;
        }
        let was_truncated = display_width(&raw_line) > MAX_LINE_LENGTH;
        if was_truncated {
            display_buffer.push_str(&truncate_with_ellipsis(&raw_line, MAX_LINE_LENGTH, "..."));
        } else {
            display_buffer.push_str(&raw_line);
        }

        if let Some(summary_line) =
            colorize_diff_summary_line(&display_buffer, renderer.capabilities().supports_color())
        {
            render_preview_line(
                renderer,
                &display_buffer,
                Some(&summary_line),
                None,
                false,
                fallback_style,
                Some(fallback_style.style()),
            )?;
            continue;
        }

        let line_style = select_line_style(tool_name, &display_buffer, git_styles, ls_styles);
        let word_bg = match line.kind {
            DiffDisplayKind::Addition => git_styles.add_word.and_then(|s| s.get_bg_color()),
            DiffDisplayKind::Deletion => git_styles.remove_word.and_then(|s| s.get_bg_color()),
            _ => None,
        };
        // Always route add/del through the dual-gutter formatter; prose falls
        // back to the solid line tint (no syntax colours).
        let rendered_owned = if color_enabled && !was_truncated {
            Some(format_diff_line_with_gutter_and_syntax(
                line,
                line_style,
                line_number_width,
                word_bg,
                &mut formatted_buffer,
            ))
        } else {
            None
        };

        render_preview_line(
            renderer,
            &display_buffer,
            rendered_owned.filter(|r| *r != display_buffer.as_str()),
            None,
            false,
            fallback_style,
            line_style,
        )?;
    }

    Ok(())
}

#[allow(
    clippy::too_many_arguments,
    reason = "Intentional compatibility, platform, or test-only suppression."
)]
#[cfg_attr(
    feature = "profiling",
    tracing::instrument(skip(renderer, content, git_styles, ls_styles, config), level = "debug")
)]
pub(crate) async fn render_stream_section(
    renderer: &mut AnsiRenderer,
    title: &str,
    content: &str,
    mode: ToolOutputMode,
    tail_limit: usize,
    tool_name: Option<&str>,
    git_styles: &GitStyles,
    ls_styles: &LsStyles,
    fallback_style: MessageStyle,
    allow_ansi: bool,
    disable_spool: bool,
    config: Option<&VTCodeConfig>,
) -> Result<()> {
    use std::fmt::Write as FmtWrite;

    let is_run_command = tool_name.is_some_and(|name| {
        tool_intent::is_command_run_tool(name)
            || name == vtcode_core::config::constants::tools::UNIFIED_EXEC
            || name == vtcode_core::config::constants::tools::EXEC_PTY_CMD
    });
    let allow_ansi_for_tool = allow_ansi && !is_run_command;
    let apply_line_styles = !is_run_command;

    // Strip ANSI codes once and reuse for both diff detection and normalization.
    // This avoids scanning the same content twice when ANSI is not allowed.
    let stripped_for_diff = strip_ansi_codes(content);
    let is_diff_content = apply_line_styles && looks_like_diff_content(stripped_for_diff.as_ref());
    let normalized_content = if allow_ansi_for_tool {
        Cow::Borrowed(content)
    } else {
        // Reuse the already-stripped result instead of scanning again.
        // Note: stripped_for_diff is consumed here, but we've already computed
        // is_diff_content above, so we use normalized_content for diff rendering below.
        stripped_for_diff
    };

    if is_run_command {
        return render_run_command_preview(
            renderer,
            normalized_content.as_ref(),
            tool_name,
            fallback_style,
            disable_spool,
            config,
        )
        .await;
    }

    // Use normalized content directly (token budget logic removed).
    // No clone needed since we own the Cow and don't use it after this point.
    let was_truncated_by_tokens = false;

    if !disable_spool
        && let Some(tool) = tool_name
        && let Ok(Some(log_path)) = spool_output_if_needed(normalized_content.as_ref(), tool, config).await
    {
        // Skip preview entirely for extremely large output
        if normalized_content.len() > EXTREME_OUTPUT_THRESHOLD_MB {
            let mut msg_buffer = String::with_capacity(256);
            let _ = write!(
                &mut msg_buffer,
                "Output too large ({} bytes), spooled to: {}",
                normalized_content.len(),
                log_path.display()
            );
            renderer.line(MessageStyle::ToolDetail, &msg_buffer)?;
            renderer.line(MessageStyle::ToolDetail, "(Preview skipped due to size)")?;
            return Ok(());
        }

        // Use compact head+tail preview (like diff view) instead of dumping tail lines
        let head_lines = RUN_COMMAND_HEAD_PREVIEW_LINES;
        let tail_lines_count = RUN_COMMAND_TAIL_PREVIEW_LINES;
        let preview = excerpt_text_lines(normalized_content.as_ref(), head_lines, tail_lines_count);
        let total = preview.total;

        let mut msg_buffer = String::with_capacity(256);
        if !is_run_command {
            let uppercase_title = if title.is_empty() {
                Cow::Borrowed("OUTPUT")
            } else {
                Cow::Owned(title.to_ascii_uppercase())
            };
            let _ = write!(
                &mut msg_buffer,
                "[{}] {} bytes, {} lines — spooled to: {}",
                uppercase_title.as_ref(),
                normalized_content.len(),
                total,
                log_path.display()
            );
        } else {
            let _ = write!(
                &mut msg_buffer,
                "{} bytes, {} lines — spooled to: {}",
                normalized_content.len(),
                total,
                log_path.display()
            );
        }
        renderer.line(MessageStyle::ToolDetail, &msg_buffer)?;

        // Render head lines
        for line in preview.head.iter() {
            render_preview_line(renderer, line, None, Some("  "), true, fallback_style, None)?;
        }

        // Show omitted notice between head and tail
        if preview.hidden_count > 0 {
            renderer.line(
                MessageStyle::ToolDetail,
                &hidden_lines_notice(preview.hidden_count, HiddenLinesNoticeKind::CommandPreview),
            )?;
        }

        // Render tail lines
        for line in preview.tail.iter() {
            render_preview_line(renderer, line, None, Some("  "), true, fallback_style, None)?;
        }

        return Ok(());
    }

    if is_diff_content {
        // Use normalized_content for diff rendering - it's already stripped when ANSI is not allowed
        render_diff_content_block(
            renderer,
            normalized_content.as_ref(),
            tool_name,
            git_styles,
            ls_styles,
            fallback_style,
            mode,
            tail_limit,
        )?;
        return Ok(());
    }

    // Token budget logic removed - use normalized content directly
    let prefer_full = renderer.prefers_untruncated_output();
    let (mut lines_vec, total, mut truncated) =
        select_stream_lines_streaming(normalized_content.as_ref(), mode, tail_limit, prefer_full);
    if prefer_full && lines_vec.len() > INLINE_STREAM_MAX_LINES {
        let drop = lines_vec.len() - INLINE_STREAM_MAX_LINES;
        lines_vec.drain(..drop);
        truncated = true;
    }

    let truncated = truncated || was_truncated_by_tokens;

    if lines_vec.is_empty() {
        return Ok(());
    }

    let mut format_buffer = String::with_capacity(64);

    let hidden = if truncated {
        total.saturating_sub(lines_vec.len())
    } else {
        0
    };
    if hidden > 0 {
        format_buffer.clear();
        format_buffer.push_str(&hidden_lines_notice(
            hidden,
            if was_truncated_by_tokens {
                HiddenLinesNoticeKind::TokenBudget
            } else {
                HiddenLinesNoticeKind::Generic
            },
        ));
        renderer.line(MessageStyle::ToolDetail, &format_buffer)?;
    }

    if should_render_as_code_block(fallback_style) && !apply_line_styles {
        let markdown = build_markdown_code_block(&lines_vec, None, true);
        renderer.render_markdown_output(fallback_style, &markdown)?;
    } else {
        for line in &lines_vec {
            if apply_line_styles && let Some(style) = select_line_style(tool_name, line, git_styles, ls_styles) {
                render_preview_line(renderer, line, None, None, true, fallback_style, Some(style))?;
            } else {
                render_preview_line(renderer, line, None, None, true, fallback_style, None)?;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use vtcode_core::ui::InlineHandle;
    use vtcode_core::utils::ansi::{AnsiRenderer, MessageStyle};

    use anstyle::AnsiColor;
    use vtcode_commons::diff_preview::{DiffDisplayKind, DiffDisplayLine};

    use crate::agent::runloop::tool_output::collect_inline_output;

    use super::{
        HiddenLinesNoticeKind, MAX_LINE_LENGTH, collect_run_command_preview, format_diff_line_with_gutter_and_syntax,
        hidden_lines_notice, highlight_diff_content, render_preview_line, strip_ansi_codes,
    };

    #[test]
    fn run_command_preview_uses_head_tail_three_lines() {
        let content = "l1\nl2\nl3\nl4\nl5\nl6\nl7\n";
        let (preview, total, hidden) = collect_run_command_preview(content);
        assert_eq!(total, 7);
        assert_eq!(hidden, 1);
        assert_eq!(preview.as_slice(), ["l1", "l2", "l3", "l5", "l6", "l7"]);
    }

    #[test]
    fn run_command_preview_keeps_short_output_unmodified() {
        let content = "l1\nl2\nl3\n";
        let (preview, total, hidden) = collect_run_command_preview(content);
        assert_eq!(total, 3);
        assert_eq!(hidden, 0);
        assert_eq!(preview.as_slice(), ["l1", "l2", "l3"]);
    }

    #[test]
    fn hidden_lines_notice_preserves_existing_variants() {
        assert_eq!(
            hidden_lines_notice(2, HiddenLinesNoticeKind::CommandPreview),
            "    … +2 lines (/share html for full transcript)"
        );
        assert_eq!(hidden_lines_notice(1, HiddenLinesNoticeKind::Generic), "[... 1 line truncated ...]");
        assert_eq!(
            hidden_lines_notice(3, HiddenLinesNoticeKind::TokenBudget),
            "[... content truncated by token budget ...]"
        );
    }

    #[test]
    fn render_preview_line_truncates_and_prefixes() {
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let mut renderer = AnsiRenderer::with_inline_ui(InlineHandle::new_for_tests(sender), Default::default());
        let line = "x".repeat(MAX_LINE_LENGTH + 10);

        render_preview_line(&mut renderer, &line, None, Some("  "), true, MessageStyle::ToolOutput, None)
            .expect("preview line should render");

        let inline_output = collect_inline_output(&mut receiver);
        assert!(inline_output.starts_with("  "));
        assert!(inline_output.ends_with("..."));
    }

    fn test_diff_line(
        kind: DiffDisplayKind,
        old_line: Option<u32>,
        new_line: Option<u32>,
        text: &str,
    ) -> DiffDisplayLine {
        DiffDisplayLine {
            kind,
            old_line,
            new_line,
            text: text.to_string(),
            changed: Vec::new(),
        }
    }

    #[test]
    fn format_diff_line_styles_gutter_for_additions() {
        let style = anstyle::Style::new().fg_color(Some(anstyle::Color::Ansi(AnsiColor::Green)));
        let mut buf = String::new();
        let rendered = format_diff_line_with_gutter_and_syntax(
            &test_diff_line(DiffDisplayKind::Addition, None, Some(1377), "let x = 1;"),
            Some(style),
            5,
            None,
            &mut buf,
        );
        assert!(rendered.contains("\u{1b}["));
        let stripped = strip_ansi_codes(rendered);
        // Blank old column (5) + ` │ ` + ` 1377` + ` │ ` + `+ content`.
        assert!(stripped.contains("+ 1377 │ let x = 1;"), "got: {stripped:?}");
    }

    #[test]
    fn format_diff_line_preserves_code_indentation() {
        let mut buf = String::new();
        let rendered = format_diff_line_with_gutter_and_syntax(
            &test_diff_line(DiffDisplayKind::Addition, None, Some(1384), "    line,"),
            None,
            5,
            None,
            &mut buf,
        );
        let stripped = strip_ansi_codes(rendered);
        assert!(stripped.contains("+ 1384 │     line,"));
    }

    #[test]
    fn format_diff_line_keeps_blank_line_spacing() {
        let mut buf = String::new();
        let rendered = format_diff_line_with_gutter_and_syntax(
            &test_diff_line(DiffDisplayKind::Addition, None, Some(42), ""),
            None,
            5,
            None,
            &mut buf,
        );
        let stripped = strip_ansi_codes(rendered);
        assert!(stripped.contains("+   42 │ "), "got: {stripped:?}");
    }

    #[test]
    fn format_diff_line_clears_reused_buffer_for_metadata() {
        let mut buf = String::new();
        let _ = format_diff_line_with_gutter_and_syntax(
            &test_diff_line(DiffDisplayKind::Addition, None, Some(1), "let x = 1;"),
            None,
            5,
            None,
            &mut buf,
        );
        let rendered = format_diff_line_with_gutter_and_syntax(
            &test_diff_line(DiffDisplayKind::Metadata, None, None, "diff --git a/src/lib.rs b/src/lib.rs"),
            None,
            5,
            None,
            &mut buf,
        );

        assert_eq!(strip_ansi_codes(rendered), "diff --git a/src/lib.rs b/src/lib.rs");
    }

    #[test]
    fn format_diff_line_keeps_markdown_bullet_distinct_from_marker() {
        let mut buf = String::new();
        let rendered = format_diff_line_with_gutter_and_syntax(
            &test_diff_line(DiffDisplayKind::Addition, None, Some(53), "- **Agent-first by design*: prose"),
            None,
            5,
            None,
            &mut buf,
        );
        let stripped = strip_ansi_codes(rendered);
        assert!(stripped.contains("+   53 │ - **Agent-first"), "got: {stripped:?}");
    }

    #[test]
    fn format_diff_line_truncates_long_addition() {
        let mut buf = String::new();
        let long_text = "y".repeat(MAX_LINE_LENGTH * 2);
        // Single gutter: sign(1) + number(5) + " │ "(3) = 9.
        let gutter_width = 9;
        let mut line = test_diff_line(DiffDisplayKind::Addition, None, Some(9), &long_text);
        // Simulate a chip that would be invalid after truncation.
        line.changed = vec![(0, long_text.len())];
        let word_bg = Some(anstyle::Color::Rgb(anstyle::RgbColor(36, 100, 70)));
        let rendered = format_diff_line_with_gutter_and_syntax(&line, None, 5, word_bg, &mut buf);
        let stripped = strip_ansi_codes(rendered);
        assert!(
            vtcode_commons::preview::display_width(&stripped) <= MAX_LINE_LENGTH + gutter_width,
            "rendered diff line must not exceed MAX_LINE_LENGTH + gutter width"
        );
        assert!(stripped.contains("..."));
        // Truncated rows must not paint word chips with stale offsets.
        assert!(!rendered.contains("48;2;36;100;70"), "no chip on truncated line");
    }

    #[test]
    fn diff_bodies_stay_solid_no_syntax_brightness() {
        let bg = Some(anstyle::Color::Rgb(anstyle::RgbColor(20, 58, 45)));
        let rendered = highlight_diff_content("- **bold** and `code`", bg, &[], None).expect("solid body");
        // Solid tint only: no bright syntax SGR, no bold, no extra fg.
        assert!(rendered.contains("48;2;20;58;45"));
        assert!(!rendered.contains("38;2;"));
        assert!(!rendered.contains("\u{1b}[1m"));
        assert!(!rendered.contains("\u{1b}[91m"));
        assert!(!rendered.contains("\u{1b}[92m"));
        assert!(highlight_diff_content("plain", None, &[], None).is_none());
    }

    #[test]
    fn word_chips_apply_stronger_bg_on_changed_spans() {
        let content = "let a = 1;";
        let word_bg = Some(anstyle::Color::Rgb(anstyle::RgbColor(36, 100, 70)));
        let line_bg = Some(anstyle::Color::Rgb(anstyle::RgbColor(20, 58, 45)));
        let rendered = highlight_diff_content(content, line_bg, &[(8, 9)], word_bg).expect("rendered");
        assert!(rendered.contains("48;2;36;100;70"));
        assert!(rendered.contains("48;2;20;58;45"));
        // Default fg on both spans: no bright syntax leak.
        assert!(!rendered.contains("38;2;"));
    }
}
