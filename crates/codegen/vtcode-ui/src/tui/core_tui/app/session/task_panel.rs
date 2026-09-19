use ratatui::prelude::*;

use super::super::super::session::{inline_list, text_utils};
use super::super::types::TaskPanelMetadata;
use crate::tui::config::constants::ui;
use vtcode_commons::ui_protocol::TaskItemStatus;

pub(super) fn compact_tree_continuation_prefix(line: &str) -> Option<String> {
    text_utils::compact_tree_continuation_prefix(line)
}

pub(super) fn wrap_line(line: &str, width: u16) -> Vec<Line<'static>> {
    if width == 0 {
        return vec![Line::from(line.to_string())];
    }

    let content = Line::from(line.to_string());
    if let Some(prefix) = compact_tree_continuation_prefix(line) {
        text_utils::wrap_line_with_hanging_prefix(content, width as usize, &prefix)
    } else {
        text_utils::wrap_line(content, width as usize)
    }
}

pub(super) fn rows(lines: &[String], width: u16, text_style: Style) -> Vec<(inline_list::InlineListRow, u16)> {
    if lines.is_empty() {
        return vec![(inline_list::InlineListRow::single(ui::PLAN_STATUS_EMPTY.to_string().into(), text_style), 1)];
    }

    lines
        .iter()
        .map(|line| {
            let wrapped = wrap_line(line, width);
            let height = inline_list::row_height(&wrapped);
            (inline_list::InlineListRow { lines: wrapped, style: text_style }, height)
        })
        .collect()
}

/// Rows with per-row styles for status text-styling.
///
/// `styles` runs parallel to `lines`; missing entries fall back to `base`.
/// Row heights derive from wrapped content exactly like [`rows`] so the
/// docked panel keeps clean wrapping with status styling applied.
pub(super) fn styled_rows(
    lines: &[String],
    styles: &[Style],
    base: Style,
    width: u16,
) -> Vec<(inline_list::InlineListRow, u16)> {
    if lines.is_empty() {
        return vec![(inline_list::InlineListRow::single(ui::PLAN_STATUS_EMPTY.to_string().into(), base), 1)];
    }

    lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            let style = styles.get(index).copied().unwrap_or(base);
            let wrapped = wrap_line(line, width);
            let height = inline_list::row_height(&wrapped);
            (inline_list::InlineListRow { lines: wrapped, style }, height)
        })
        .collect()
}

/// Per-row style for a task status under the theme tokens.
///
/// Done rows dim, italicize, and strike through on the base style; the
/// focused row and other in-progress rows use the `primary` accent (focused
/// adds bold); blocked rows use the `warning` token; pending rows keep the
/// base style. All colors flow through the shared style bridge.
pub(super) fn row_style(status: TaskItemStatus, is_current: bool, base: Style) -> Style {
    match status {
        TaskItemStatus::Completed => base.add_modifier(Modifier::DIM | Modifier::ITALIC | Modifier::CROSSED_OUT),
        TaskItemStatus::Blocked => {
            crate::tui::core_tui::style::ratatui_style_from_ansi(crate::theme::active_styles().warning)
        }
        TaskItemStatus::InProgress if is_current => {
            crate::tui::core_tui::style::ratatui_style_from_ansi(crate::theme::active_styles().primary)
                .add_modifier(Modifier::BOLD)
        }
        TaskItemStatus::InProgress => {
            crate::tui::core_tui::style::ratatui_style_from_ansi(crate::theme::active_styles().primary)
        }
        TaskItemStatus::Pending => base,
    }
}

/// Align panel body inputs after the legacy header strip.
///
/// Returns the visible body slice with statuses/current shifted by the same
/// skipped prefix so per-row styling stays attached to the right row.
pub(super) fn aligned_body<'a>(
    lines: &'a [String],
    statuses: &'a [TaskItemStatus],
    current: Option<usize>,
    metadata: Option<&TaskPanelMetadata>,
) -> (&'a [String], &'a [TaskItemStatus], Option<usize>) {
    let body = body_lines(lines, metadata);
    let skipped = lines.len().saturating_sub(body.len());
    if statuses.len() != lines.len() {
        return (body, &[], None);
    }
    let aligned_statuses = &statuses[skipped.min(statuses.len())..];
    let aligned_current = current
        .and_then(|index| index.checked_sub(skipped))
        .filter(|index| *index < body.len());
    (body, aligned_statuses, aligned_current)
}

pub(super) fn body_lines<'a>(lines: &'a [String], metadata: Option<&TaskPanelMetadata>) -> &'a [String] {
    let Some(metadata) = metadata else {
        return lines;
    };

    let Some(first) = lines.first() else {
        return lines;
    };

    // Panel body receives compact tree rows only. Defensive strip of legacy
    // transcript headers keeps old fixtures/render paths from duplicating
    // title/progress under the typed panel header.
    if first.starts_with("• Tasks") {
        return &lines[1..];
    }

    first
        .strip_prefix("• ")
        .filter(|title| *title == metadata.title)
        .map_or(lines, |_| &lines[1..])
}

pub(super) fn row_count(lines: &[String], width: u16) -> usize {
    rows(lines, width, Style::default())
        .iter()
        .map(|(_, height)| *height as usize)
        .sum::<usize>()
        .max(1)
}

pub(super) fn header(metadata: Option<&TaskPanelMetadata>, item_count: usize) -> (String, String) {
    let title = metadata
        .map(|metadata| metadata.title.as_str())
        .filter(|title| !title.trim().is_empty())
        .unwrap_or(ui::PLAN_BLOCK_TITLE)
        .to_string();
    let progress = metadata
        .map(|metadata| format!("{}/{} complete", metadata.completed, metadata.total))
        .unwrap_or_else(|| format!("{} item{}", item_count, if item_count == 1 { "" } else { "s" }));
    (title, progress)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row_text(row: &inline_list::InlineListRow) -> Vec<String> {
        row.lines
            .iter()
            .map(|line| line.spans.iter().map(|span| span.content.as_ref()).collect())
            .collect()
    }

    #[test]
    fn compact_tree_continuations_hang_two_spaces_beyond_tree_indent() {
        assert_eq!(compact_tree_continuation_prefix("  └ Parent"), Some("    ".to_string()));
        assert_eq!(compact_tree_continuation_prefix("    [-] Child"), Some("      ".to_string()));
        assert_eq!(compact_tree_continuation_prefix("• Updated Plan"), None);
    }

    #[test]
    fn wrapped_tree_rows_keep_hanging_indentation() {
        let rows = rows(&["    [-] A child task with a long description".to_string()], 18, Style::default());
        assert_eq!(rows.len(), 1);
        assert!(row_text(&rows[0].0)[1].starts_with("      "));
        assert_eq!(rows[0].1, 4);
    }

    #[test]
    fn row_count_accounts_for_wrapped_tree_rows() {
        let lines = vec![
            "  └ A short parent".to_string(),
            "    □ A much longer child task".to_string(),
        ];
        assert_eq!(row_count(&lines, 80), 2);
        assert!(row_count(&lines, 12) > 2);
    }

    #[test]
    fn header_uses_typed_title_and_completion_progress() {
        let metadata = TaskPanelMetadata {
            title: "Release".to_string(),
            completed: 2,
            total: 5,
        };
        assert_eq!(header(Some(&metadata), 8), ("Release".to_string(), "2/5 complete".to_string()));
        assert_eq!(header(None, 1), (ui::PLAN_BLOCK_TITLE.to_string(), "1 item".to_string()));
    }

    #[test]
    fn body_lines_uses_typed_header_instead_of_duplicate_title_row() {
        let lines = vec!["• Release".to_string(), "  └ Prepare release".to_string()];
        let metadata = TaskPanelMetadata {
            title: "Release".to_string(),
            completed: 0,
            total: 1,
        };

        assert_eq!(body_lines(&lines, Some(&metadata)), &lines[1..]);
        assert_eq!(body_lines(&lines, None), &lines[..]);
    }

    #[test]
    fn body_lines_strips_summary_header_since_panel_shows_progress() {
        let lines = vec![
            "• Tasks 1/4 — next: Prepare release".to_string(),
            "  └ □ Prepare release".to_string(),
        ];
        let metadata = TaskPanelMetadata {
            title: "Release".to_string(),
            completed: 1,
            total: 4,
        };

        assert_eq!(body_lines(&lines, Some(&metadata)), &lines[1..]);
    }

    #[test]
    fn rows_keep_the_existing_empty_state() {
        let empty_rows = rows(&[], 40, Style::default());
        assert_eq!(empty_rows.len(), 1);
        assert_eq!(row_text(&empty_rows[0].0), vec![ui::PLAN_STATUS_EMPTY.to_string()]);
        assert_eq!(empty_rows[0].1, 1);
    }

    #[test]
    fn row_style_maps_status_to_theme_text_styling() {
        use ratatui::style::Modifier;

        let base = Style::default();
        let done = row_style(TaskItemStatus::Completed, false, base);
        assert!(done.add_modifier.contains(Modifier::CROSSED_OUT), "done rows strike through");
        assert!(done.add_modifier.contains(Modifier::ITALIC), "done rows italicize");
        assert!(done.add_modifier.contains(Modifier::DIM), "done rows dim");
        assert_eq!(done.fg, base.fg, "done rows keep the base hue");

        let pending = row_style(TaskItemStatus::Pending, false, base);
        assert_eq!(pending, base, "pending rows keep the base style");

        let active = row_style(TaskItemStatus::InProgress, false, base);
        let current = row_style(TaskItemStatus::InProgress, true, base);
        assert_ne!(active.fg, base.fg, "in-progress rows use the accent");
        assert_eq!(active.fg, current.fg, "focused and plain in-progress share the accent hue");
        assert!(current.add_modifier.contains(Modifier::BOLD), "focused row is bold");
        assert!(!active.add_modifier.contains(Modifier::BOLD), "non-focused rows stay unbolded");

        let blocked = row_style(TaskItemStatus::Blocked, false, base);
        assert_ne!(blocked.fg, base.fg, "blocked rows use the warning token");
        assert_ne!(blocked.fg, active.fg, "warning stays distinct from the progress accent");
    }

    #[test]
    fn styled_rows_keep_text_with_per_row_styles_and_base_fallback() {
        let lines = vec![
            "  ├ Defer eager setup".to_string(),
            "  └ Verify with cargo check".to_string(),
        ];
        let styles = vec![
            row_style(TaskItemStatus::InProgress, true, Style::default()),
            row_style(TaskItemStatus::Completed, false, Style::default()),
        ];

        let styled = styled_rows(&lines, &styles, Style::default(), 80);

        assert_eq!(styled.len(), 2);
        assert_eq!(row_text(&styled[0].0), vec!["  ├ Defer eager setup".to_string()]);
        assert_eq!(styled[0].0.style, styles[0]);
        assert_eq!(styled[1].0.style, styles[1]);
        assert_eq!(styled[0].1, 1);
        // Missing entries fall back to the base style instead of blank.
        let fallback = styled_rows(&lines, &[], Style::default(), 80);
        assert_eq!(fallback[0].0.style, Style::default());
        assert_eq!(fallback[1].0.style, Style::default());
    }

    #[test]
    fn aligned_body_shifts_statuses_with_legacy_header_strip() {
        use TaskItemStatus::{Blocked, Completed, Pending};

        let lines = vec![
            "• Release".to_string(),
            "  ├ Defer eager setup".to_string(),
            "  └ Verify with cargo check".to_string(),
        ];
        let statuses = vec![Pending, Blocked, Completed];
        let metadata = TaskPanelMetadata {
            title: "Release".to_string(),
            completed: 1,
            total: 3,
        };

        let (body, aligned, current) = aligned_body(&lines, &statuses, Some(1), Some(&metadata));

        assert_eq!(body, &lines[1..]);
        assert_eq!(aligned, &[Blocked, Completed]);
        assert_eq!(current, Some(0));

        // Length mismatches (legacy callers) degrade to uniform styling.
        let (body, aligned, current) = aligned_body(&lines, &[], Some(0), Some(&metadata));
        assert_eq!(body, &lines[1..]);
        assert!(aligned.is_empty());
        assert_eq!(current, None);
    }
}
