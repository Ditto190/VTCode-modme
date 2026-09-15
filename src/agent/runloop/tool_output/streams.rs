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

use anstyle::{AnsiColor, Reset, Style as AnsiStyle};
use anyhow::Result;
use smallvec::SmallVec;
use vtcode_commons::preview::{
    display_width, excerpt_text_lines, format_hidden_lines_summary as shared_hidden_lines_summary,
    truncate_with_ellipsis,
};
use vtcode_core::config::ToolOutputMode;
use vtcode_core::config::loader::VTCodeConfig;
use vtcode_core::tools::tool_intent;
use vtcode_core::utils::ansi::{AnsiRenderer, MessageStyle};
use vtcode_diff::{
    DiffDisplayKind, DiffDisplayLine, SideBySideRow, bounded_display_lines, diff_display_line_number_width,
    diff_gutter_fits, diff_gutter_width, diff_side_by_side_fits, display_lines_from_unified_diff, side_by_side_rows,
};

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
    highlight_diff_content_with_foreground(content, bg, word_ranges, word_bg, None)
}

fn highlight_diff_content_with_foreground(
    content: &str,
    bg: Option<anstyle::Color>,
    word_ranges: &[(usize, usize)],
    word_bg: Option<anstyle::Color>,
    foreground: Option<anstyle::Color>,
) -> Option<String> {
    // ANSI16/no-color mode passes `bg: None`, so there is no tint or word
    // chip to paint. Return `None` so the caller falls through to the
    // foreground fallback body style.
    if content.is_empty() {
        return None;
    }
    let bg = bg?;
    let mut out = String::with_capacity(content.len() + 16);
    // Reset first so no prior SGR state bleeds into this run.
    out.push_str(&Reset.to_string());

    if word_ranges.is_empty() || word_bg.is_none() {
        out.push_str(&AnsiStyle::new().fg_color(foreground).bg_color(Some(bg)).render().to_string());
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
            out.push_str(&AnsiStyle::new().fg_color(foreground).bg_color(Some(bg)).render().to_string());
            out.push_str(&content[cursor..start]);
            out.push_str(&Reset.to_string());
        }
        if end > start {
            out.push_str(
                &AnsiStyle::new()
                    .fg_color(foreground)
                    .bg_color(Some(word_bg))
                    .render()
                    .to_string(),
            );
            out.push_str(&content[start..end]);
            out.push_str(&Reset.to_string());
        }
        cursor = end.max(cursor);
    }
    if cursor < content.len() {
        out.push_str(&AnsiStyle::new().fg_color(foreground).bg_color(Some(bg)).render().to_string());
        out.push_str(&content[cursor..]);
        out.push_str(&Reset.to_string());
    }
    Some(out)
}

fn semantic_diff_line_style(
    line: &DiffDisplayLine,
    style: Option<AnsiStyle>,
    git_styles: &GitStyles,
) -> Option<AnsiStyle> {
    let mut style = style?;
    if style.get_fg_color().is_none() {
        let foreground = match line.kind {
            DiffDisplayKind::Addition => Some(git_styles.addition_fg),
            DiffDisplayKind::Deletion => Some(git_styles.deletion_fg),
            _ => None,
        };
        if let Some(foreground) = foreground {
            style = style.fg_color(Some(foreground));
        }
    }
    Some(style)
}

fn diff_display_text(line: &DiffDisplayLine, line_number_width: usize, show_gutter: bool) -> String {
    if show_gutter || !line.kind.is_diff() {
        return line.numbered_text(line_number_width);
    }

    // Keep one styled leading cell as the compact row marker. It preserves
    // diff-row identity in the inline transcript without spending columns on
    // +/- signs, line numbers, or the vertical separator.
    if line.text.is_empty() {
        "  ".to_owned()
    } else {
        format!(" {}", line.text)
    }
}

fn select_render_line_style(
    line: &DiffDisplayLine,
    display_line: &str,
    tool_name: Option<&str>,
    git_styles: &GitStyles,
    ls_styles: &LsStyles,
) -> Option<AnsiStyle> {
    select_line_style_for_kind(line, git_styles).or_else(|| {
        // In the compact layout context rows start with a single blank, so a
        // source line beginning with `+` or `-` must not be reclassified as an
        // insertion or deletion by the generic parser.
        (!line.kind.is_diff())
            .then(|| select_line_style(tool_name, display_line, git_styles, ls_styles))
            .flatten()
    })
}

fn should_show_diff_gutter(
    color_enabled: bool,
    has_row_background: bool,
    available_width: Option<usize>,
    line_number_width: usize,
) -> bool {
    !color_enabled
        || !has_row_background
        || available_width.is_none_or(|width| diff_gutter_fits(width, line_number_width))
}

#[cfg(test)]
fn format_diff_line_with_gutter_and_syntax<'a>(
    line: &DiffDisplayLine,
    base_style: Option<AnsiStyle>,
    line_number_width: usize,
    word_bg: Option<anstyle::Color>,
    git_styles: &GitStyles,
    out: &'a mut String,
) -> &'a str {
    format_diff_line_with_gutter_and_syntax_to_width(
        line,
        base_style,
        line_number_width,
        word_bg,
        git_styles,
        None,
        true,
        out,
    )
}

fn format_diff_line_with_gutter_and_syntax_to_width<'a>(
    line: &DiffDisplayLine,
    base_style: Option<AnsiStyle>,
    line_number_width: usize,
    word_bg: Option<anstyle::Color>,
    git_styles: &GitStyles,
    target_width: Option<usize>,
    show_gutter: bool,
    out: &'a mut String,
) -> &'a str {
    use std::fmt::Write as _;

    out.clear();
    let (marker, mut content) = match line.kind {
        DiffDisplayKind::Addition => ('+', line.text.as_str()),
        DiffDisplayKind::Deletion => ('-', line.text.as_str()),
        DiffDisplayKind::Context => (' ', line.text.as_str()),
        DiffDisplayKind::Metadata | DiffDisplayKind::HunkHeader => {
            let max_width = target_width.map_or(MAX_LINE_LENGTH, |width| width.min(MAX_LINE_LENGTH));
            let text = line.numbered_text(line_number_width);
            let text = if display_width(&text) > max_width {
                truncate_with_ellipsis(&text, max_width, "...")
            } else {
                text
            };
            if let Some(style) = base_style {
                out.push_str(&style.render().to_string());
            }
            out.push_str(&text);
            out.push_str(&Reset.to_string());
            return out;
        }
    };
    if content.is_empty() {
        content = " ";
    }

    // Keep the complete rendered row within the measured width when one is
    // available; otherwise retain the generic preview cap.
    let prefix_width: usize = if show_gutter { 4 + line_number_width } else { 1 };
    let max_width = target_width.map_or(MAX_LINE_LENGTH, |width| width.min(MAX_LINE_LENGTH));
    let content_width = max_width.saturating_sub(prefix_width);
    let content_owned;
    let mut truncated = false;
    let content: &str = if display_width(content) > content_width {
        content_owned = truncate_with_ellipsis(content, content_width, "...");
        truncated = true;
        &content_owned
    } else {
        content
    };

    let bg = base_style.and_then(|style| style.get_bg_color());
    // Unified gutter: the sign uses the shared accessible marker foreground;
    // numbers use a theme-aware muted foreground. The body uses the row tint
    // while changed spans receive the stronger word background.
    let marker_style = match marker {
        '+' => AnsiStyle::new().fg_color(Some(git_styles.addition_fg)).bg_color(bg),
        '-' => AnsiStyle::new().fg_color(Some(git_styles.deletion_fg)).bg_color(bg),
        _ => AnsiStyle::new().fg_color(Some(git_styles.gutter_fg)),
    };
    let gutter_style = AnsiStyle::new().fg_color(Some(git_styles.gutter_fg)).bg_color(bg);
    let reset = Reset;
    out.reserve(line.text.len() + 32);
    // Single gutter: `sign + number + │ + content`. The `│` keeps markdown
    // bullets (`- foo`) distinct from the diff marker (`+`/`-`).
    // Reset between spans so SGR state never leaks into the next. The outer
    // line style deliberately remains active for the message indent, making
    // the row tint continuous from the transcript prefix through the gutter.
    let line_no = match marker {
        '+' => line.new_line,
        '-' => line.old_line,
        _ => line.new_line.or(line.old_line),
    }
    .unwrap_or_default();
    let mut body_style = base_style.unwrap_or_else(|| AnsiStyle::new().bg_color(bg));
    if !show_gutter && body_style.get_fg_color().is_none() {
        let foreground = match marker {
            '+' => Some(git_styles.addition_fg),
            '-' => Some(git_styles.deletion_fg),
            _ => None,
        };
        if let Some(foreground) = foreground {
            body_style = body_style.fg_color(Some(foreground));
        }
    }

    if show_gutter {
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
    } else {
        // One blank, styled cell keeps compact rows identifiable to the
        // inline transcript's diff reflow while dropping the visible gutter.
        let compact_marker = AnsiStyle::new().bg_color(bg);
        let _ = write!(out, "{compact_marker} ");
        let _ = write!(out, "{Reset}");
    }
    // Gutter is 1 (sign) + number_width + 3 (` │ `); compact rows use one
    // marker cell only.
    // Color-capable rows use a neutral foreground on the row tint; ANSI16
    // falls back to the bright body foreground. Skip chips on truncated rows — `changed` offsets are into the original
    // text and would highlight the wrong slice after ellipsis truncation.
    let word_ranges: &[(usize, usize)] = if matches!(marker, '+' | '-') && !content.is_empty() && !truncated {
        &line.changed
    } else {
        &[]
    };
    let highlighted = highlight_diff_content_with_foreground(
        content,
        bg,
        word_ranges,
        word_bg,
        (!show_gutter).then(|| body_style.get_fg_color()).flatten(),
    );
    if let Some(highlighted) = highlighted {
        out.push_str(&highlighted);
    } else {
        let _ = write!(out, "{}", body_style.render());
        out.push_str(content);
        let _ = write!(out, "{reset}");
    }

    // Continue the base row tint through the unused cells on the right. Word
    // chips above remain stronger because padding is appended only after the
    // body has restored the base row state.
    if let Some(bg) = bg
        && let Some(target_width) = target_width
    {
        let visible_width = prefix_width.saturating_add(display_width(content));
        let padding = target_width.saturating_sub(visible_width);
        if padding > 0 {
            let _ = write!(out, "{}{}{}", AnsiStyle::new().bg_color(Some(bg)).render(), " ".repeat(padding), reset);
        }
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
    let effective_limit = if renderer.prefers_untruncated_output() || matches!(mode, ToolOutputMode::Full) {
        tail_limit.max(1000)
    } else {
        tail_limit
    };
    let bounded_lines = bounded_display_lines(&diff_lines, effective_limit);
    let lines_slice = bounded_lines.as_slice();
    let line_number_width = diff_display_line_number_width(lines_slice);
    let available_width = renderer.diff_content_width(fallback_style);

    if renderer.diff_preview_mode() == vtcode_commons::ui_protocol::DiffPreviewMode::SideBySide
        && diff_side_by_side_fits(available_width)
    {
        return render_diff_content_side_by_side(renderer, lines_slice, git_styles, fallback_style);
    }

    // Without ANSI styling the row tint and foreground fallback disappear,
    // so keep the marker/line-number gutter as the only remaining add/delete
    // distinction even when the content width is narrow.
    let has_row_background = git_styles.add.as_ref().is_some_and(|style| style.get_bg_color().is_some());
    let show_gutter = should_show_diff_gutter(
        renderer.capabilities().supports_color(),
        has_row_background,
        available_width,
        line_number_width,
    );
    render_diff_content_inline(
        renderer,
        lines_slice,
        tool_name,
        git_styles,
        ls_styles,
        fallback_style,
        line_number_width,
        show_gutter,
    )
}

fn render_diff_content_inline(
    renderer: &mut AnsiRenderer,
    lines_slice: &[DiffDisplayLine],
    tool_name: Option<&str>,
    git_styles: &GitStyles,
    ls_styles: &LsStyles,
    fallback_style: MessageStyle,
    line_number_width: usize,
    show_gutter: bool,
) -> Result<()> {
    let color_enabled = renderer.capabilities().supports_color();
    let target_width = renderer.diff_content_width(fallback_style);
    // A measured content width is a stricter bound than the generic preview
    // cap. Compact rows must not wrap in the transcript after their gutter is
    // hidden on a narrow terminal.
    let max_line_width = target_width.map_or(MAX_LINE_LENGTH, |width| width.min(MAX_LINE_LENGTH));
    let mut formatted_buffer = String::with_capacity(256);
    let mut display_buffer = String::with_capacity(256);

    for line in lines_slice {
        display_buffer.clear();
        let raw_line = diff_display_text(line, line_number_width, show_gutter);
        if raw_line.is_empty() {
            continue;
        }
        if display_width(&raw_line) > max_line_width {
            display_buffer.push_str(&truncate_with_ellipsis(&raw_line, max_line_width, "..."));
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

        let line_style = select_render_line_style(line, &display_buffer, tool_name, git_styles, ls_styles);
        let line_style = semantic_diff_line_style(line, line_style, git_styles);
        let word_bg = match line.kind {
            DiffDisplayKind::Addition => git_styles.add_word.and_then(|s| s.get_bg_color()),
            DiffDisplayKind::Deletion => git_styles.remove_word.and_then(|s| s.get_bg_color()),
            _ => None,
        };
        // Route add/delete rows through the shared formatter; compact layouts
        // omit the visible gutter while prose falls back to the solid tint.
        let rendered_owned = if color_enabled
            && (!show_gutter || target_width.is_none_or(|width| width >= diff_gutter_width(line_number_width)))
        {
            Some(format_diff_line_with_gutter_and_syntax_to_width(
                line,
                line_style,
                line_number_width,
                word_bg,
                git_styles,
                target_width,
                show_gutter,
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

/// Render diff display lines as dual-pane (old | new) rows.
///
/// Context lines appear on both sides. Consecutive deletion/addition runs are
/// zipped index-wise. Hunk headers and metadata span the full width.
fn render_diff_content_side_by_side(
    renderer: &mut AnsiRenderer,
    lines_slice: &[DiffDisplayLine],
    git_styles: &GitStyles,
    fallback_style: MessageStyle,
) -> Result<()> {
    let rows = side_by_side_rows(lines_slice);
    let line_number_width = diff_display_line_number_width(lines_slice);
    let color_enabled = renderer.capabilities().supports_color();
    let mut formatted_buffer = String::with_capacity(512);
    let mut display_buffer = String::with_capacity(512);

    // Target pane width: leave room for gutter + divider within the actual
    // content width when terminal sizing is available. Keep the bounded
    // fallback for non-terminal tests and redirected output.
    let total_width = renderer.diff_content_width(fallback_style).unwrap_or(MAX_LINE_LENGTH);
    let pane_width = ((total_width.saturating_sub(3)) / 2).max(20);
    let full_width_limit = total_width.min(MAX_LINE_LENGTH);

    for row in rows {
        display_buffer.clear();
        let raw_line = format_side_by_side_row_plain(&row, line_number_width, pane_width);
        if raw_line.is_empty() {
            continue;
        }
        // Source rows are already bounded independently by `pane_width`; do
        // not apply the single-line preview cap to the combined panes on a
        // wide terminal or the right pane would be clipped before styling.
        let was_truncated = row.is_full_width() && display_width(&raw_line) > full_width_limit;
        if was_truncated {
            display_buffer.push_str(&truncate_with_ellipsis(&raw_line, full_width_limit, "..."));
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

        // Side-by-side rows carry their own per-pane ANSI backgrounds.
        // Pass a bg-less override so an empty left/right cell cannot inherit
        // the sibling pane's tint via the line-level style.
        let rendered_owned = if color_enabled && !was_truncated {
            Some(format_side_by_side_row_ansi(&row, line_number_width, pane_width, git_styles, &mut formatted_buffer))
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
            Some(AnsiStyle::new()),
        )?;
    }

    Ok(())
}

/// Plain-text dual-pane row for fallback / truncation.
fn format_side_by_side_row_plain(row: &SideBySideRow, number_width: usize, pane_width: usize) -> String {
    if row.is_full_width() {
        return row.left.as_ref().map(|l| l.text.clone()).unwrap_or_default();
    }
    let left = pane_cell_plain(row.left.as_ref(), number_width, pane_width);
    let right = pane_cell_plain(row.right.as_ref(), number_width, pane_width);
    format!("{left}│{right}")
}

fn pane_cell_plain(line: Option<&DiffDisplayLine>, number_width: usize, pane_width: usize) -> String {
    let Some(line) = line else {
        return " ".repeat(pane_width);
    };
    let (marker, number) = match line.kind {
        DiffDisplayKind::Addition => ('+', line.new_line),
        DiffDisplayKind::Deletion => ('-', line.old_line),
        _ => (' ', line.new_line.or(line.old_line)),
    };
    let no = number.map(|n| n.to_string()).unwrap_or_default();
    let gutter = format!("{marker}{no:>number_width$}│");
    let body_width = pane_width.saturating_sub(gutter.chars().count());
    let body = truncate_chars_to_width(&line.text, body_width);
    let pad = body_width.saturating_sub(display_width(&body));
    format!("{gutter}{body}{}", " ".repeat(pad))
}

/// ANSI-styled dual-pane row with per-pane backgrounds.
fn format_side_by_side_row_ansi<'a>(
    row: &SideBySideRow,
    number_width: usize,
    pane_width: usize,
    git_styles: &GitStyles,
    out: &'a mut String,
) -> &'a str {
    out.clear();
    use std::fmt::Write as _;
    let reset_background = diff_background_reset(git_styles);

    if row.is_full_width() {
        let text = row.left.as_ref().map(|l| l.text.as_str()).unwrap_or("");
        let _ = write!(out, "{Reset}");
        let style = row.left.as_ref().and_then(|line| match line.kind {
            DiffDisplayKind::HunkHeader => git_styles.hunk,
            DiffDisplayKind::Metadata if line.text.starts_with("--- ") => git_styles.file_old,
            DiffDisplayKind::Metadata if line.text.starts_with("+++ ") => git_styles.file_new,
            DiffDisplayKind::Metadata => git_styles.header,
            _ => None,
        });
        if let Some(style) = style {
            let _ = write!(out, "{style}");
        }
        out.push_str(text);
        let _ = write!(out, "{Reset}");
        return out.as_str();
    }

    let left = format_side_by_side_pane_ansi(row.left.as_ref(), number_width, pane_width, git_styles);
    let right = format_side_by_side_pane_ansi(row.right.as_ref(), number_width, pane_width, git_styles);
    // Start the row with a full reset + default bg so nothing carries over
    // from the previous line.
    out.push_str(reset_background);
    out.push_str(&left);
    let divider = AnsiStyle::new().fg_color(Some(anstyle::Color::Ansi(AnsiColor::BrightBlack)));
    let _ = write!(out, "{divider}│{reset_background}");
    out.push_str(&right);
    // End with full reset so the next line starts clean.
    let _ = write!(out, "\x1b[0m");
    out.as_str()
}

fn diff_background_reset(git_styles: &GitStyles) -> &'static str {
    if git_styles.add.as_ref().is_some_and(|style| style.get_bg_color().is_some()) {
        "\x1b[0m\x1b[49m"
    } else {
        "\x1b[0m"
    }
}

fn format_side_by_side_pane_ansi(
    line: Option<&DiffDisplayLine>,
    number_width: usize,
    pane_width: usize,
    git_styles: &GitStyles,
) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(pane_width + 16);
    let Some(line) = line else {
        // SGR 49 = default background. More reliable than SGR 0 (full reset)
        // for clearing an inherited tint in color-capable terminal parsers;
        // ANSI16 stays foreground-only and needs only the full reset.
        out.push_str(diff_background_reset(git_styles));
        out.push_str(&" ".repeat(pane_width));
        return out;
    };
    let base_style = select_line_style_for_kind(line, git_styles);
    let bg = base_style.and_then(|s| s.get_bg_color());
    let word_bg = match line.kind {
        DiffDisplayKind::Addition => git_styles.add_word.and_then(|s| s.get_bg_color()),
        DiffDisplayKind::Deletion => git_styles.remove_word.and_then(|s| s.get_bg_color()),
        _ => None,
    };
    let (marker, number) = match line.kind {
        DiffDisplayKind::Addition => ('+', line.new_line),
        DiffDisplayKind::Deletion => ('-', line.old_line),
        _ => (' ', line.new_line.or(line.old_line)),
    };
    let no = number.map(|n| n.to_string()).unwrap_or_default();

    // One continuous tint through the whole pane: marker + number + │ + body
    // + pad. No Reset between cells — that would carve out an unstyled gutter
    // strip next to the coloured body.
    let marker_style = match marker {
        '+' => AnsiStyle::new().fg_color(Some(git_styles.addition_fg)).bg_color(bg),
        '-' => AnsiStyle::new().fg_color(Some(git_styles.deletion_fg)).bg_color(bg),
        _ => AnsiStyle::new().fg_color(Some(git_styles.gutter_fg)).bg_color(bg),
    };
    // Avoid DIM: its attenuation is terminal-dependent and can fail contrast
    // against the stronger intraline background.
    let gutter_style = AnsiStyle::new().fg_color(Some(git_styles.gutter_fg)).bg_color(bg);
    // Color-capable rows use the row tint and reserve the stronger tint for
    // changed spans. ANSI16 uses the bright body foreground fallback.
    let body_style = match marker {
        '+' => git_styles.add.unwrap_or_default(),
        '-' => git_styles.remove.unwrap_or_default(),
        _ => AnsiStyle::new().bg_color(bg),
    };

    let _ = write!(out, "{marker_style}{marker}");
    let _ = write!(out, "{gutter_style}{no:>number_width$}│");
    // Gutter is 1 (sign) + number_width + 1 (│).
    let body_width = pane_width.saturating_sub(2 + number_width);
    let truncated = display_width(&line.text) > body_width;
    let body = truncate_chars_to_width(&line.text, body_width);
    // Skip chips on truncated rows — `changed` offsets are into the original
    // text and would highlight the wrong slice after truncation.
    let word_ranges: &[(usize, usize)] = if truncated { &[] } else { &line.changed };
    let highlighted = highlight_diff_content(&body, bg, word_ranges, word_bg);
    match highlighted {
        Some(hl) => {
            out.push_str(&hl);
        }
        None => {
            let _ = write!(out, "{body_style}{body}");
        }
    }
    // Pad to exact remaining cells so the pane stays aligned.
    // Track used width in display cells, not chars, to handle wide glyphs.
    let mut used = 1 + number_width + 1; // sign + number + │
    for ch in body.chars() {
        used += unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1);
    }
    let pad = pane_width.saturating_sub(used);
    if pad > 0 {
        let _ = write!(out, "{body_style}{}", " ".repeat(pad));
    }
    // End the pane with a reset so the next pane starts clean.
    let _ = write!(out, "{Reset}");
    out
}

/// Truncate to at most `max_width` display cells without an ellipsis marker.
fn truncate_chars_to_width(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    let mut out = String::with_capacity(text.len());
    let mut used = 0usize;
    for ch in text.chars() {
        let w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1);
        if used + w > max_width {
            break;
        }
        out.push(ch);
        used += w;
    }
    out
}

fn select_line_style_for_kind(line: &DiffDisplayLine, git_styles: &GitStyles) -> Option<AnsiStyle> {
    match line.kind {
        DiffDisplayKind::Addition => git_styles.add,
        DiffDisplayKind::Deletion => git_styles.remove,
        _ => None,
    }
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
    use vtcode_commons::diff_theme::{DiffColorLevel, DiffTheme};
    use vtcode_diff::{DiffDisplayKind, DiffDisplayLine, SideBySideRow};

    use crate::agent::runloop::tool_output::collect_inline_output;

    use super::super::styles::{GitStyles, LsStyles};
    use super::{
        HiddenLinesNoticeKind, MAX_LINE_LENGTH, collect_run_command_preview, format_diff_line_with_gutter_and_syntax,
        format_diff_line_with_gutter_and_syntax_to_width, format_side_by_side_row_ansi, hidden_lines_notice,
        highlight_diff_content, render_diff_content_block, render_preview_line, select_render_line_style,
        should_show_diff_gutter, strip_ansi_codes,
    };
    use vtcode_core::config::ToolOutputMode;

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

    #[test]
    fn diff_stream_preview_keeps_head_tail_and_omission_marker() {
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let mut renderer = AnsiRenderer::with_inline_ui(InlineHandle::new_for_tests(sender), Default::default());
        let mut diff = String::from("@@ -1,600 +1,600 @@\n");
        for index in 0..600 {
            diff.push_str(&format!("-old-{index}\n+new-{index}\n"));
        }

        render_diff_content_block(
            &mut renderer,
            &diff,
            Some("apply_patch"),
            &GitStyles::new(),
            &LsStyles::from_env(),
            MessageStyle::ToolDetail,
            ToolOutputMode::Compact,
            5,
        )
        .expect("bounded diff should render");

        let collected = collect_inline_output(&mut receiver);
        let output = strip_ansi_codes(&collected);
        assert!(output.contains("old-0"), "head should be retained: {output:?}");
        assert!(output.contains("new-599"), "tail should be retained: {output:?}");
        assert!(output.contains("lines omitted"), "omission marker should be rendered: {output:?}");
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
            &GitStyles::new(),
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
            &GitStyles::new(),
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
            &GitStyles::new(),
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
            &GitStyles::new(),
            &mut buf,
        );
        let rendered = format_diff_line_with_gutter_and_syntax(
            &test_diff_line(DiffDisplayKind::Metadata, None, None, "diff --git a/src/lib.rs b/src/lib.rs"),
            None,
            5,
            None,
            &GitStyles::new(),
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
            &GitStyles::new(),
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
        let rendered = format_diff_line_with_gutter_and_syntax(&line, None, 5, word_bg, &GitStyles::new(), &mut buf);
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

    #[test]
    fn inline_diff_uses_shared_accessible_marker_gutter_and_two_level_palette() {
        let git = GitStyles::new_for(DiffTheme::Dark, DiffColorLevel::TrueColor);
        let mut line = test_diff_line(DiffDisplayKind::Addition, None, Some(7), "let value = 2;");
        line.changed = vec![(4, 9)];
        let mut buffer = String::new();
        let rendered = format_diff_line_with_gutter_and_syntax(
            &line,
            git.add,
            3,
            git.add_word.and_then(|style| style.get_bg_color()),
            &git,
            &mut buffer,
        )
        .to_owned();

        assert!(rendered.contains("48;2;20;58;45"), "row tint missing: {rendered:?}");
        assert!(rendered.contains("48;2;36;100;70"), "word tint missing: {rendered:?}");
        assert!(rendered.contains("38;2;85;255;85"), "accessible addition marker missing: {rendered:?}");
        assert!(rendered.contains("38;2;165;175;170"), "accessible gutter missing: {rendered:?}");
    }

    #[test]
    fn inline_diff_row_tint_reaches_the_requested_width() {
        let git = GitStyles::new_for(DiffTheme::Dark, DiffColorLevel::TrueColor);
        let line = test_diff_line(DiffDisplayKind::Addition, None, Some(7), "let value = 2;");
        let mut buffer = String::new();
        let rendered = format_diff_line_with_gutter_and_syntax_to_width(
            &line,
            git.add,
            3,
            git.add_word.and_then(|style| style.get_bg_color()),
            &git,
            Some(48),
            true,
            &mut buffer,
        );
        let stripped = strip_ansi_codes(rendered);

        assert_eq!(vtcode_commons::preview::display_width(&stripped), 48);
        assert!(rendered.contains("48;2;20;58;45"), "row tint missing: {rendered:?}");
        assert!(rendered.ends_with("\x1b[0m"), "padded row must reset cleanly: {rendered:?}");
    }

    #[test]
    fn compact_diff_row_drops_the_visible_gutter_but_keeps_semantic_styling() {
        let git = GitStyles::new_for(DiffTheme::Dark, DiffColorLevel::TrueColor);
        let mut line = test_diff_line(DiffDisplayKind::Addition, None, Some(7), "changed");
        line.changed = vec![(0, line.text.len())];
        let mut buffer = String::new();
        let rendered = format_diff_line_with_gutter_and_syntax_to_width(
            &line,
            git.add,
            5,
            git.add_word.and_then(|style| style.get_bg_color()),
            &git,
            Some(28),
            false,
            &mut buffer,
        );
        let stripped = strip_ansi_codes(rendered);

        assert_eq!(vtcode_commons::preview::display_width(&stripped), 28);
        assert!(stripped.starts_with(' '));
        assert!(!stripped.contains('+'));
        assert!(!stripped.contains('│'));
        assert!(!stripped.contains('7'));
        assert!(rendered.contains("48;2;20;58;45"), "compact row tint missing: {rendered:?}");
        assert!(rendered.contains("38;2;85;255;85"), "compact semantic foreground missing: {rendered:?}");
        assert!(rendered.contains("48;2;36;100;70"), "compact word tint missing: {rendered:?}");
    }

    #[test]
    fn compact_diff_row_truncates_before_applying_color() {
        let git = GitStyles::new_for(DiffTheme::Dark, DiffColorLevel::TrueColor);
        let line = test_diff_line(DiffDisplayKind::Addition, None, Some(7), &"changed ".repeat(20));
        let mut buffer = String::new();
        let rendered = format_diff_line_with_gutter_and_syntax_to_width(
            &line,
            git.add,
            5,
            git.add_word.and_then(|style| style.get_bg_color()),
            &git,
            Some(28),
            false,
            &mut buffer,
        );
        let stripped = strip_ansi_codes(rendered);

        assert_eq!(vtcode_commons::preview::display_width(&stripped), 28);
        assert!(rendered.contains("48;2;20;58;45"), "truncated row tint missing: {rendered:?}");
        assert!(rendered.contains("38;2;85;255;85"), "compact semantic foreground missing: {rendered:?}");
    }

    #[test]
    fn compact_context_content_is_not_reclassified_as_a_deletion() {
        let git = GitStyles::new_for(DiffTheme::Dark, DiffColorLevel::TrueColor);
        let ls = LsStyles::from_env();
        let line = test_diff_line(DiffDisplayKind::Context, Some(4), Some(4), "- bullet item");

        assert_eq!(
            select_render_line_style(&line, " - bullet item", Some("run_pty_cmd"), &git, &ls),
            None,
            "compact context rows must keep source-leading markers as content"
        );
    }

    #[test]
    fn foreground_only_diff_rows_keep_their_visible_gutter_when_narrow() {
        assert!(should_show_diff_gutter(true, false, Some(1), 5));
        assert!(!should_show_diff_gutter(true, true, Some(28), 5));
        assert!(should_show_diff_gutter(true, true, Some(29), 5));
    }

    #[test]
    fn narrow_inline_diff_rows_fit_the_measured_content_width() {
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let mut renderer = AnsiRenderer::with_inline_ui(InlineHandle::new_for_tests(sender), Default::default());
        renderer.set_diff_preview_mode(vtcode_commons::ui_protocol::DiffPreviewMode::Inline);
        renderer.set_table_max_width(Some(28));
        let git = GitStyles::new_for(DiffTheme::Dark, DiffColorLevel::TrueColor);
        let diff = format!("@@ -1 +1 @@\n-{}\n+{}\n", "old ".repeat(30), "new ".repeat(30));

        render_diff_content_block(
            &mut renderer,
            &diff,
            Some("apply_patch"),
            &git,
            &LsStyles::from_env(),
            MessageStyle::ToolDetail,
            ToolOutputMode::Full,
            100,
        )
        .expect("narrow diff should render");

        let collected = collect_inline_output(&mut receiver);
        let output = strip_ansi_codes(&collected);
        assert!(
            output.lines().all(|line| {
                let content = line.strip_prefix(MessageStyle::ToolDetail.indent()).unwrap_or(line);
                vtcode_commons::preview::display_width(content) <= 20
            }),
            "rows must fit the measured content width after indentation: {output:?}"
        );
    }

    #[test]
    fn side_by_side_diff_falls_back_narrow_and_recovers_wide() {
        fn render(mode: vtcode_commons::ui_protocol::DiffPreviewMode, width: usize) -> String {
            let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
            let mut renderer = AnsiRenderer::with_inline_ui(InlineHandle::new_for_tests(sender), Default::default());
            renderer.set_diff_preview_mode(mode);
            renderer.set_table_max_width(Some(width));
            let git = GitStyles::new_for(DiffTheme::Dark, DiffColorLevel::TrueColor);
            render_diff_content_block(
                &mut renderer,
                "--- a/file.rs\n+++ b/file.rs\n@@ -1 +1 @@\n-old value\n+new value\n",
                Some("apply_patch"),
                &git,
                &LsStyles::from_env(),
                MessageStyle::ToolDetail,
                ToolOutputMode::Full,
                100,
            )
            .expect("diff preview should render");
            strip_ansi_codes(&collect_inline_output(&mut receiver)).into_owned()
        }

        assert_eq!(
            render(vtcode_commons::ui_protocol::DiffPreviewMode::SideBySide, 50),
            render(vtcode_commons::ui_protocol::DiffPreviewMode::Inline, 50),
            "narrow side-by-side should use the inline renderer"
        );
        assert_ne!(
            render(vtcode_commons::ui_protocol::DiffPreviewMode::SideBySide, 80),
            render(vtcode_commons::ui_protocol::DiffPreviewMode::Inline, 80),
            "wide side-by-side should recover the configured layout"
        );
    }

    #[test]
    fn wide_side_by_side_rows_are_not_clipped_to_inline_preview_limit() {
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let mut renderer = AnsiRenderer::with_inline_ui(InlineHandle::new_for_tests(sender), Default::default());
        renderer.set_diff_preview_mode(vtcode_commons::ui_protocol::DiffPreviewMode::SideBySide);
        renderer.set_table_max_width(Some(200));
        let git = GitStyles::new_for(DiffTheme::Dark, DiffColorLevel::TrueColor);
        let old = format!("-{}\n", "old ".repeat(40));
        let new = format!("+{}\n", "new ".repeat(40));
        let diff = format!("@@ -1 +1 @@\n{old}{new}");

        render_diff_content_block(
            &mut renderer,
            &diff,
            Some("apply_patch"),
            &git,
            &LsStyles::from_env(),
            MessageStyle::ToolDetail,
            ToolOutputMode::Full,
            100,
        )
        .expect("wide side-by-side diff should render");

        let collected = collect_inline_output(&mut receiver);
        let output = strip_ansi_codes(&collected);
        assert!(
            output
                .lines()
                .any(|line| vtcode_commons::preview::display_width(line) > MAX_LINE_LENGTH),
            "source panes should use the measured width instead of the inline cap: {output:?}"
        );
    }

    #[test]
    fn side_by_side_metadata_fits_the_measured_content_width() {
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let mut renderer = AnsiRenderer::with_inline_ui(InlineHandle::new_for_tests(sender), Default::default());
        renderer.set_diff_preview_mode(vtcode_commons::ui_protocol::DiffPreviewMode::SideBySide);
        renderer.set_table_max_width(Some(80));
        let git = GitStyles::new_for(DiffTheme::Dark, DiffColorLevel::TrueColor);
        let path = "very-long-directory-name/another-long-directory-name/source-file.rs";
        let diff = format!("diff --git a/{path} b/{path}\n@@ -1 +1 @@\n-old\n+new\n");

        render_diff_content_block(
            &mut renderer,
            &diff,
            Some("apply_patch"),
            &git,
            &LsStyles::from_env(),
            MessageStyle::ToolDetail,
            ToolOutputMode::Full,
            100,
        )
        .expect("side-by-side metadata should render");

        let collected = collect_inline_output(&mut receiver);
        let output = strip_ansi_codes(&collected);
        assert!(
            output.lines().all(|line| {
                let content = line.strip_prefix(MessageStyle::ToolDetail.indent()).unwrap_or(line);
                vtcode_commons::preview::display_width(content) <= 72
            }),
            "full-width rows must fit measured content width: {output:?}"
        );
    }

    #[test]
    fn diff_section_headers_are_foreground_only_and_highlighted() {
        let git = GitStyles::new_for(DiffTheme::Dark, DiffColorLevel::TrueColor);
        let mut buffer = String::new();

        let file_header = format_diff_line_with_gutter_and_syntax(
            &test_diff_line(DiffDisplayKind::Metadata, None, None, "--- a/README.md"),
            git.file_old,
            3,
            None,
            &git,
            &mut buffer,
        )
        .to_owned();
        assert!(!file_header.contains("48;"), "file header must not have a background: {file_header:?}");
        assert!(file_header.contains("38;2;255;180;180"), "file header foreground missing: {file_header:?}");

        let hunk_header = format_diff_line_with_gutter_and_syntax(
            &test_diff_line(DiffDisplayKind::HunkHeader, None, None, "@@ -1 +1 @@"),
            git.hunk,
            3,
            None,
            &git,
            &mut buffer,
        )
        .to_owned();
        assert!(!hunk_header.contains("48;"), "hunk header must not have a background: {hunk_header:?}");
        assert!(hunk_header.contains("36m"), "hunk header foreground missing: {hunk_header:?}");
    }

    #[test]
    fn side_by_side_diff_resets_empty_pane_and_keeps_tints_isolated() {
        let git = GitStyles::new_for(DiffTheme::Dark, DiffColorLevel::TrueColor);
        let mut deleted = test_diff_line(DiffDisplayKind::Deletion, Some(3), None, "old value");
        deleted.changed = vec![(4, 9)];
        let mut added = test_diff_line(DiffDisplayKind::Addition, None, Some(3), "new value");
        added.changed = vec![(4, 9)];

        let mut buffer = String::new();
        let left_only =
            format_side_by_side_row_ansi(&SideBySideRow { left: Some(deleted), right: None }, 2, 24, &git, &mut buffer)
                .to_owned();
        assert!(left_only.contains("48;2;70;38;42"));
        assert!(left_only.contains("48;2;140;52;58"));
        assert!(!left_only.contains("48;2;20;58;45"));
        assert!(left_only.contains("\x1b[0m\x1b[49m"), "empty right pane must reset its background");

        let right_only =
            format_side_by_side_row_ansi(&SideBySideRow { left: None, right: Some(added) }, 2, 24, &git, &mut buffer)
                .to_owned();
        assert!(right_only.contains("48;2;20;58;45"));
        assert!(right_only.contains("48;2;36;100;70"));
        assert!(!right_only.contains("48;2;70;38;42"));
        assert!(right_only.starts_with("\x1b[0m\x1b[49m"), "empty left pane must reset its background");
    }

    #[test]
    fn ansi16_side_by_side_diff_stays_foreground_only() {
        let git = GitStyles::new_for(DiffTheme::Dark, DiffColorLevel::Ansi16);
        let added = test_diff_line(DiffDisplayKind::Addition, None, Some(3), "new value");
        let mut buffer = String::new();
        let rendered =
            format_side_by_side_row_ansi(&SideBySideRow { left: None, right: Some(added) }, 2, 24, &git, &mut buffer);

        assert!(!rendered.contains("48;"), "ANSI16 must not paint a background: {rendered:?}");
        assert!(!rendered.contains("49m"), "ANSI16 must not emit background resets: {rendered:?}");
        assert!(rendered.contains("\x1b[92m"), "ANSI16 foreground should remain visible: {rendered:?}");
    }
}
