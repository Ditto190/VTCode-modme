//! Diff preview rendering for file edit approval
//!
//! Renders a syntax-highlighted diff preview with permission controls.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use vtcode_commons::diff_paths::language_hint_from_path;
use vtcode_commons::diff_preview::{
    DiffDisplayKind, DiffDisplayLine, count_diff_changes, display_lines_from_hunks, side_by_side_rows,
};
use vtcode_commons::ui_protocol::DiffPreviewMode as DiffLayoutMode;

use super::Session;
use crate::tui::core_tui::app::types::{DiffPreviewMode, DiffPreviewState, TrustMode};
use crate::tui::core_tui::style::ratatui_color_from_ansi;
use crate::tui::utils::diff::{DiffBundle, DiffOptions, compute_diff_with_theme};
use crate::tui::utils::diff_styles::{
    DiffColorPalette, DiffLineType, current_diff_render_style_context, style_content, style_gutter, style_hunk_header,
    style_line_bg, style_sign,
};

/// Below this width, side-by-side falls back to the single-column view.
const MIN_SIDE_BY_SIDE_WIDTH: u16 = 60;

pub(crate) fn render_diff_preview(session: &Session, frame: &mut Frame<'_>, area: Rect) {
    let Some(preview) = session.diff_preview_state() else {
        return;
    };

    // Clear the full overlay area so empty side-by-side cells cannot show
    // through to the transcript underneath.
    frame.render_widget(Clear, area);

    let palette = DiffColorPalette::default();
    let diff_bundle = compute_diff_with_theme(
        &preview.before,
        &preview.after,
        DiffOptions {
            context_lines: 3,
            old_label: None,
            new_label: None,
            missing_newline_hint: false,
        },
    );
    let counts = count_diff_changes(&diff_bundle.hunks);

    let [header, content, controls] = area
        .try_layout(&Layout::vertical([Constraint::Length(2), Constraint::Min(5), Constraint::Length(4)]))
        .unwrap_or([Rect::ZERO; 3]);

    let layout_mode = session.core.appearance.diff_preview_mode;
    render_file_header(frame, header, preview, &palette, counts.additions, counts.deletions, layout_mode);
    match layout_mode {
        DiffLayoutMode::SideBySide if content.width >= MIN_SIDE_BY_SIDE_WIDTH => {
            render_diff_content_side_by_side(frame, content, preview, &diff_bundle);
        }
        _ => render_diff_content(frame, content, preview, &diff_bundle),
    }
    render_controls(frame, controls, preview);
}

fn render_file_header(
    frame: &mut Frame<'_>,
    area: Rect,
    preview: &DiffPreviewState,
    palette: &DiffColorPalette,
    additions: usize,
    deletions: usize,
    layout_mode: DiffLayoutMode,
) {
    let header_style = Style::default().fg(ratatui_color_from_ansi(palette.header_fg));
    let mut spans = vec![
        Span::styled(header_action_label(preview.mode), header_style),
        Span::styled(&preview.file_path, header_style),
        Span::styled(" (", header_style),
        Span::styled(format!("+{additions}"), Style::default().fg(ratatui_color_from_ansi(palette.added_fg))),
        Span::styled(" ", header_style),
        Span::styled(format!("-{deletions}"), Style::default().fg(ratatui_color_from_ansi(palette.removed_fg))),
        Span::styled(")", header_style),
    ];
    if layout_mode == DiffLayoutMode::SideBySide {
        spans.push(Span::styled("  [side-by-side]", Style::default().fg(Color::DarkGray)));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_diff_content(frame: &mut Frame<'_>, area: Rect, preview: &DiffPreviewState, diff_bundle: &DiffBundle) {
    let language = language_hint_from_path(&preview.file_path);
    let style_context = current_diff_render_style_context();
    let width = area.width as usize;

    let mut lines: Vec<Line> = Vec::new();
    let max_display = area.height.saturating_sub(1) as usize;
    let display_lines = display_lines_from_hunks(&diff_bundle.hunks);

    for display_line in display_lines {
        if lines.len() >= max_display {
            break;
        }

        match display_line.kind {
            DiffDisplayKind::HunkHeader => {
                let header_style = style_hunk_header(style_context);
                lines.push(pad_line_to_width(
                    Line::from(Span::styled(display_line.text, header_style)).style(header_style),
                    width,
                    header_style,
                ));
            }
            DiffDisplayKind::Metadata => {
                lines.push(Line::from(Span::styled(display_line.text, Style::default().fg(Color::DarkGray))));
            }
            DiffDisplayKind::Context | DiffDisplayKind::Addition | DiffDisplayKind::Deletion => {
                lines.push(build_inline_diff_line(&display_line, language.as_deref(), style_context, width));
            }
        }
    }

    if lines.is_empty() {
        lines.push(Line::from(Span::styled("(no changes)", Style::default().fg(Color::DarkGray))));
    }

    frame.render_widget(Paragraph::new(lines).block(Block::default().borders(Borders::NONE)), area);
}

fn render_diff_content_side_by_side(
    frame: &mut Frame<'_>,
    area: Rect,
    preview: &DiffPreviewState,
    diff_bundle: &DiffBundle,
) {
    let language = language_hint_from_path(&preview.file_path);
    let style_context = current_diff_render_style_context();

    // Split into three independent areas so pane backgrounds physically
    // cannot bleed across the divider. Left gets floor((w-1)/2), right gets
    // the rest — together with the 1-col divider they always fill `area`.
    let total_w = area.width.max(3);
    let left_w = (total_w.saturating_sub(1)) / 2;
    let right_w = total_w.saturating_sub(left_w + 1);
    let left_area = Rect {
        x: area.x,
        y: area.y,
        width: left_w,
        height: area.height,
    };
    let divider_area = Rect {
        x: area.x + left_w,
        y: area.y,
        width: 1,
        height: area.height,
    };
    let right_area = Rect {
        x: area.x + left_w + 1,
        y: area.y,
        width: right_w,
        height: area.height,
    };

    // Column headers row (top 1 row of the content area).
    let header_h = 1u16.min(area.height);
    let header_left = Rect { height: header_h, ..left_area };
    let header_div = Rect { height: header_h, ..divider_area };
    let header_right = Rect { height: header_h, ..right_area };
    let body_left = Rect {
        y: area.y + header_h,
        height: area.height.saturating_sub(header_h),
        ..left_area
    };
    let body_div = Rect {
        y: area.y + header_h,
        height: area.height.saturating_sub(header_h),
        ..divider_area
    };
    let body_right = Rect {
        y: area.y + header_h,
        height: area.height.saturating_sub(header_h),
        ..right_area
    };

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled("Old", Style::default().fg(Color::DarkGray)))),
        header_left,
    );
    frame
        .render_widget(Paragraph::new(Line::from(Span::styled("│", Style::default().fg(Color::DarkGray)))), header_div);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled("New", Style::default().fg(Color::DarkGray)))),
        header_right,
    );

    let display_lines = display_lines_from_hunks(&diff_bundle.hunks);
    let rows = side_by_side_rows(&display_lines);
    let max_display = body_left.height as usize;

    let mut left_lines: Vec<Line> = Vec::new();
    let mut right_lines: Vec<Line> = Vec::new();
    let mut full_width_lines: Vec<(usize, Line)> = Vec::new();

    for row in rows.iter() {
        if left_lines.len() >= max_display {
            break;
        }

        if row.is_full_width() {
            let full = row.left.as_ref().expect("full-width row stores the band on left");
            let line = if full.kind == DiffDisplayKind::HunkHeader {
                let header_style = style_hunk_header(style_context);
                pad_line_to_width(
                    Line::from(Span::styled(full.text.clone(), header_style)).style(header_style),
                    area.width as usize,
                    header_style,
                )
            } else {
                Line::from(Span::styled(full.text.clone(), Style::default().fg(Color::DarkGray)))
            };
            // Track full-width rows separately; both panes show a blank spacer.
            full_width_lines.push((left_lines.len(), line));
            // Line::default() has no spans — paints nothing, buffer stays default.
            left_lines.push(Line::default());
            right_lines.push(Line::default());
            continue;
        }

        // Left pane
        match &row.left {
            Some(line) if line.kind.is_diff() => {
                left_lines.push(build_side_pane_line(line, language.as_deref(), style_context, left_w as usize));
            }
            _ => {
                // Empty left: no spans — terminal default bg, no bleed.
                left_lines.push(Line::default());
            }
        }

        // Right pane
        match &row.right {
            Some(line) if line.kind.is_diff() => {
                right_lines.push(build_side_pane_line(line, language.as_deref(), style_context, right_w as usize));
            }
            _ => {
                right_lines.push(Line::default());
            }
        }
    }

    if left_lines.is_empty() {
        left_lines.push(Line::from(Span::styled("(no changes)", Style::default().fg(Color::DarkGray))));
    }

    // Render panes independently — no shared Line means no bg bleed.
    // The divider needs one row per body line; a single-Line Paragraph would
    // only paint the top row and leave the rest of the column blank.
    let divider_lines: Vec<Line> = (0..body_div.height)
        .map(|_| Line::from(Span::styled("│", Style::default().fg(Color::DarkGray))))
        .collect();
    frame.render_widget(Paragraph::new(left_lines), body_left);
    frame.render_widget(Paragraph::new(divider_lines), body_div);
    frame.render_widget(Paragraph::new(right_lines), body_right);

    // Overlay full-width bands (hunk headers) on top of both panes.
    for (row_index, line) in full_width_lines {
        let y = body_left.y + row_index as u16;
        if y >= body_left.y + body_left.height {
            continue;
        }
        let band = Rect { x: area.x, y, width: area.width, height: 1 };
        frame.render_widget(Paragraph::new(line), band);
    }
}

/// Build a single pane line with foreground-only styling (gutter + content + pad).
fn build_side_pane_line(
    display_line: &DiffDisplayLine,
    _language: Option<&str>,
    style_context: crate::tui::utils::diff_styles::DiffRenderStyleContext,
    width: usize,
) -> Line<'static> {
    let mut spans = build_side_pane_spans(display_line, _language, style_context, width);
    // Foreground-only: ensure no background leaks through. Gutter DIM is
    // intentional (numbers recede); bodies carry no DIM so add/del fg stays
    // at full brightness aligned with the marker.
    for span in &mut spans {
        span.style.bg = None;
    }
    let line_bg = style_line_bg(line_type_for_kind(display_line.kind), style_context);
    pad_line_to_width(Line::from(spans), width, line_bg)
}

fn line_type_for_kind(kind: DiffDisplayKind) -> DiffLineType {
    match kind {
        DiffDisplayKind::Addition => DiffLineType::Insert,
        DiffDisplayKind::Deletion => DiffLineType::Delete,
        _ => DiffLineType::Context,
    }
}

fn build_inline_diff_line(
    display_line: &DiffDisplayLine,
    _language: Option<&str>,
    style_context: crate::tui::utils::diff_styles::DiffRenderStyleContext,
    width: usize,
) -> Line<'static> {
    let line_type = if display_line.kind == DiffDisplayKind::Context {
        DiffLineType::Context
    } else if display_line.kind == DiffDisplayKind::Addition {
        DiffLineType::Insert
    } else {
        DiffLineType::Delete
    };

    let gutter_style = style_gutter(line_type, style_context);
    let sign_style = style_sign(line_type, style_context);
    let line_bg = style_line_bg(line_type, style_context);
    let content_style = style_content(line_type, style_context);

    let prefix = match line_type {
        DiffLineType::Insert => "+",
        DiffLineType::Delete => "-",
        DiffLineType::Context => " ",
    };

    // Single gutter: `sign + number + │`.
    let line_no = match display_line.kind {
        DiffDisplayKind::Deletion => display_line.old_line,
        DiffDisplayKind::Addition => display_line.new_line,
        _ => display_line.new_line.or(display_line.old_line),
    };
    let line_num_str = match line_no {
        Some(n) => format!("{n:>4}"),
        None => "    ".to_owned(),
    };
    let mut spans = vec![
        Span::styled(prefix.to_owned(), sign_style),
        Span::styled(line_num_str, gutter_style),
        Span::styled(" │ ".to_owned(), gutter_style),
    ];

    // Foreground-only bodies: solid red/green foreground aligned with the
    // sign marker. No syntax highlighting here so code and prose share one
    // fg and no theme background can leave holes.
    spans.push(Span::styled(display_line.text.clone(), content_style));

    pad_line_to_width(Line::from(spans).style(line_bg), width, line_bg)
}

/// Build one side-by-side pane: `sign + number + │ + content`, truncated to `width`.
fn build_side_pane_spans(
    display_line: &DiffDisplayLine,
    _language: Option<&str>,
    style_context: crate::tui::utils::diff_styles::DiffRenderStyleContext,
    width: usize,
) -> Vec<Span<'static>> {
    let line_type = if display_line.kind == DiffDisplayKind::Context {
        DiffLineType::Context
    } else if display_line.kind == DiffDisplayKind::Addition {
        DiffLineType::Insert
    } else {
        DiffLineType::Delete
    };

    let gutter_style = style_gutter(line_type, style_context);
    let sign_style = style_sign(line_type, style_context);
    let content_style = style_content(line_type, style_context);

    let prefix = match line_type {
        DiffLineType::Insert => "+",
        DiffLineType::Delete => "-",
        DiffLineType::Context => " ",
    };
    let line_no = match display_line.kind {
        DiffDisplayKind::Deletion => display_line.old_line,
        DiffDisplayKind::Addition => display_line.new_line,
        _ => display_line.new_line.or(display_line.old_line),
    };
    let line_num_str = match line_no {
        Some(n) => format!("{n:>3}"),
        None => "   ".to_owned(),
    };

    let mut spans = vec![
        Span::styled(prefix.to_owned(), sign_style),
        Span::styled(line_num_str.clone(), gutter_style),
        Span::styled("│".to_owned(), gutter_style),
    ];
    // Gutter width grows with large line numbers (`{n:>3}` widens past 3
    // digits), so measure it instead of assuming 5 cells.
    let used = 1usize
        .saturating_add(unicode_width::UnicodeWidthStr::width(line_num_str.as_str()))
        .saturating_add(1);

    // Foreground-only: solid red/green body, truncated to the pane width.
    // No syntax highlighting so the body fg stays aligned with the marker.
    if used < width {
        let body = truncate_to_width(&display_line.text, width - used);
        let cell_width = unicode_width::UnicodeWidthStr::width(body.as_str());
        spans.push(Span::styled(body, content_style));
        let _ = cell_width;
    }

    // Empty pane content still needs a cell so the divider stays aligned.
    if used < width && spans.len() == 3 {
        spans.push(Span::styled(" ".repeat(width - used), content_style));
    }

    spans
}

fn truncate_to_width(text: &str, max_width: usize) -> String {
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

/// Pad a diff row with tinted spaces so the background spans the full width.
///
/// `Paragraph` only paints behind actual spans; without padding the tint
/// ends at the last character and short rows look striped.
fn pad_line_to_width(mut line: Line<'static>, width: usize, bg_style: Style) -> Line<'static> {
    if width == 0 {
        return line;
    }
    let line_width: usize = line
        .spans
        .iter()
        .map(|span| {
            span.content
                .chars()
                .map(|ch| unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1))
                .sum::<usize>()
        })
        .sum();
    let padding = width.saturating_sub(line_width);
    if padding == 0 {
        return line;
    }
    let pad_bg = bg_style.bg.map(|bg| Style::default().bg(bg)).unwrap_or_default();
    line.spans.push(Span::styled(" ".repeat(padding), pad_bg));
    line
}

fn header_action_label(mode: DiffPreviewMode) -> &'static str {
    match mode {
        DiffPreviewMode::EditApproval => "← Edit ",
        DiffPreviewMode::FileConflict => "← Conflict ",
        DiffPreviewMode::ReadonlyReview => "← Review ",
    }
}

fn render_controls(frame: &mut Frame<'_>, area: Rect, preview: &DiffPreviewState) {
    let lines = control_lines(preview);

    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::TOP)
                .border_style(Style::default().fg(Color::DarkGray)),
        ),
        area,
    );
}

fn control_lines(preview: &DiffPreviewState) -> Vec<Line<'static>> {
    let action_key_style = |color: Color| -> Style { Style::default().fg(color).add_modifier(Modifier::BOLD) };

    let key_green = Color::LightGreen;
    let key_red = Color::LightRed;
    let key_cyan = Color::LightCyan;
    let key_yellow = Color::Yellow;
    let muted = Color::Gray;

    match preview.mode {
        DiffPreviewMode::EditApproval => {
            let trust = match preview.trust_mode {
                TrustMode::Once => "Once",
                TrustMode::Session => "Session",
                TrustMode::Always => "Always",
                TrustMode::AutoTrust => "Auto",
            };

            vec![
                Line::from(vec![
                    Span::styled("Enter", action_key_style(key_green)),
                    Span::raw(" Apply  "),
                    Span::styled("Esc", action_key_style(key_red)),
                    Span::raw(" Reject  "),
                    Span::styled("Tab", action_key_style(key_yellow)),
                    Span::raw("/"),
                    Span::styled("S-Tab", action_key_style(key_yellow)),
                    Span::raw(" Nav"),
                ]),
                Line::from(vec![
                    Span::styled("1", action_key_style(key_cyan)),
                    Span::raw("-Once "),
                    Span::styled("2", action_key_style(key_cyan)),
                    Span::raw("-Sess "),
                    Span::styled("3", action_key_style(key_cyan)),
                    Span::raw("-Always "),
                    Span::styled("4", action_key_style(key_cyan)),
                    Span::raw("-Auto "),
                    Span::styled(format!("[{trust}]"), Style::default().fg(muted).add_modifier(Modifier::BOLD)),
                ]),
            ]
        }
        DiffPreviewMode::FileConflict => vec![
            Line::from(vec![
                Span::styled("Enter", action_key_style(key_green)),
                Span::raw(" Proceed  "),
                Span::styled("r", action_key_style(key_cyan)),
                Span::raw(" Reload  "),
                Span::styled("Esc", action_key_style(key_red)),
                Span::raw(" Abort"),
            ]),
            Line::from(vec![
                Span::styled("Tab", action_key_style(key_yellow)),
                Span::raw("/"),
                Span::styled("S-Tab", action_key_style(key_yellow)),
                Span::raw(" Nav"),
            ]),
        ],
        DiffPreviewMode::ReadonlyReview => vec![
            Line::from(vec![
                Span::styled("Enter", action_key_style(key_green)),
                Span::raw(" Back  "),
                Span::styled("Esc", action_key_style(key_red)),
                Span::raw(" Back"),
            ]),
            Line::from(vec![
                Span::styled("Tab", action_key_style(key_yellow)),
                Span::raw("/"),
                Span::styled("S-Tab", action_key_style(key_yellow)),
                Span::raw(" Nav"),
            ]),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::{control_lines, header_action_label};
    use crate::tui::core_tui::app::types::{DiffPreviewMode, DiffPreviewState};

    #[test]
    fn conflict_controls_show_proceed_reload_abort_copy() {
        let preview = DiffPreviewState::new_with_mode(
            "src/main.rs".to_string(),
            "before".to_string(),
            "after".to_string(),
            Vec::new(),
            DiffPreviewMode::FileConflict,
        );

        let lines = control_lines(&preview);
        let first_line: String = lines[0].spans.iter().map(|span| span.content.clone().into_owned()).collect();

        assert!(first_line.contains("Proceed"));
        assert!(first_line.contains("Reload"));
        assert!(first_line.contains("Abort"));
    }

    #[test]
    fn readonly_review_controls_show_back_navigation() {
        let preview = DiffPreviewState::new_with_mode(
            "src/main.rs".to_string(),
            "before".to_string(),
            "after".to_string(),
            Vec::new(),
            DiffPreviewMode::ReadonlyReview,
        );

        let lines = control_lines(&preview);
        let first_line: String = lines[0].spans.iter().map(|span| span.content.clone().into_owned()).collect();

        assert!(first_line.contains("Back"));
        assert!(!first_line.contains("Proceed"));
        assert!(!first_line.contains("Reload"));
    }

    #[test]
    fn conflict_header_uses_conflict_label() {
        assert_eq!(header_action_label(DiffPreviewMode::FileConflict), "← Conflict ");
        assert_eq!(header_action_label(DiffPreviewMode::EditApproval), "← Edit ");
        assert_eq!(header_action_label(DiffPreviewMode::ReadonlyReview), "← Review ");
    }
}
