use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, Paragraph},
};
use ratatui_cheese::input::{Input, InputState};
use unicode_width::UnicodeWidthChar;

use super::{Session, ToolOutputBlock};
use crate::tui::config::constants::ui;
use crate::tui::core_tui::session::list_panel::input_styles_from_theme;

#[derive(Clone, Debug, Default)]
struct ToolOutputSearchState {
    active: bool,
    pending_query: String,
    query: String,
    matches: Vec<usize>,
    current_match: Option<usize>,
    restore_scroll_top: usize,
    restore_query: String,
    restore_match: Option<usize>,
}

#[derive(Clone, Debug, Default)]
struct CachedToolOutputBlock {
    revision: u64,
    lines: Vec<String>,
    lowered_lines: Option<Vec<String>>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ToolOutputViewerState {
    width: u16,
    source_revision: u64,
    messages: Vec<CachedToolOutputBlock>,
    row_offsets: Vec<usize>,
    total_lines: usize,
    cached_export_text: Option<String>,
    scroll_top: usize,
    search: ToolOutputSearchState,
}

impl ToolOutputViewerState {
    pub(crate) fn open(session: &Session, width: u16, height: u16) -> Self {
        let mut state = Self::default();
        state.refresh(session, width, height);
        state.scroll_to_bottom(height);
        state
    }

    pub(crate) fn refresh(&mut self, session: &Session, width: u16, height: u16) {
        let width = width.max(1);
        let revision = session.tool_output_revision;
        if self.width == width && self.source_revision == revision {
            self.clamp_scroll(height);
            return;
        }

        let was_at_bottom = self.is_at_bottom(height);
        self.refresh_messages(session, width);
        self.width = width;
        self.source_revision = revision;
        self.recompute_matches();

        if was_at_bottom {
            self.scroll_to_bottom(height);
        } else {
            self.clamp_scroll(height);
        }
    }

    fn line_count(&self) -> usize {
        self.total_lines.max(1)
    }

    pub(crate) fn export_text(&mut self) -> String {
        if let Some(text) = &self.cached_export_text {
            return text.clone();
        }

        let mut export = String::new();
        let mut wrote_line = false;
        for message in &self.messages {
            for line in &message.lines {
                if wrote_line {
                    export.push('\n');
                }
                export.push_str(line);
                wrote_line = true;
            }
        }

        self.cached_export_text = Some(export.clone());
        export
    }

    fn visible_lines(&self, height: usize) -> Vec<Line<'static>> {
        let height = height.max(1);
        let end = self.scroll_top.saturating_add(height).min(self.total_lines);
        let current_match_line = self.current_match_line();
        let mut visible = Vec::with_capacity(height);

        for row in self.scroll_top..end {
            let style = if current_match_line == Some(row) {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            let line = self.line_text_at(row).map_or_else(String::new, str::to_string);
            visible.push(Line::styled(line, style));
        }

        while visible.len() < height {
            visible.push(Line::default());
        }

        visible
    }

    pub(crate) fn scroll_line_up(&mut self, height: u16) {
        self.scroll_top = self.scroll_top.saturating_sub(1);
        self.clamp_scroll(height);
    }

    pub(crate) fn scroll_line_down(&mut self, height: u16) {
        self.scroll_top = self.scroll_top.saturating_add(1).min(self.max_scroll(height));
    }

    pub(crate) fn scroll_half_page_up(&mut self, height: u16) {
        self.scroll_top = self.scroll_top.saturating_sub(Self::page_step(height).max(1) / 2);
        self.clamp_scroll(height);
    }

    pub(crate) fn scroll_half_page_down(&mut self, height: u16) {
        self.scroll_top = self
            .scroll_top
            .saturating_add(Self::page_step(height).max(1) / 2)
            .min(self.max_scroll(height));
    }

    pub(crate) fn scroll_full_page_up(&mut self, height: u16) {
        self.scroll_top = self.scroll_top.saturating_sub(Self::page_step(height));
        self.clamp_scroll(height);
    }

    pub(crate) fn scroll_full_page_down(&mut self, height: u16) {
        self.scroll_top = self
            .scroll_top
            .saturating_add(Self::page_step(height))
            .min(self.max_scroll(height));
    }

    pub(crate) fn scroll_to_top(&mut self) {
        self.scroll_top = 0;
    }

    pub(crate) fn scroll_to_bottom(&mut self, height: u16) {
        self.scroll_top = self.max_scroll(height);
    }

    pub(crate) fn start_search(&mut self) {
        if self.search.active {
            return;
        }
        self.search.active = true;
        self.search.pending_query = self.search.query.clone();
        self.search.restore_scroll_top = self.scroll_top;
        self.search.restore_query = self.search.query.clone();
        self.search.restore_match = self.search.current_match;
    }

    pub(crate) fn search_active(&self) -> bool {
        self.search.active
    }

    fn search_query(&self) -> &str {
        if self.search.active {
            &self.search.pending_query
        } else {
            &self.search.query
        }
    }

    pub(crate) fn insert_search_text(&mut self, text: &str) {
        self.search.pending_query.push_str(text);
    }

    pub(crate) fn backspace_search(&mut self) {
        self.search.pending_query.pop();
    }

    pub(crate) fn cancel_search(&mut self) {
        self.search.active = false;
        self.scroll_top = self.search.restore_scroll_top;
        self.search.query = self.search.restore_query.clone();
        self.search.current_match = self.search.restore_match;
        self.search.pending_query.clear();
        self.recompute_matches();
    }

    pub(crate) fn commit_search(&mut self, height: u16) {
        self.search.active = false;
        self.search.query = std::mem::take(&mut self.search.pending_query);
        self.recompute_matches();
        if !self.search.matches.is_empty() {
            self.search.current_match = Some(0);
            self.jump_to_current_match(height);
        } else {
            self.search.current_match = None;
        }
    }

    pub(crate) fn jump_next_match(&mut self, height: u16) {
        if self.search.matches.is_empty() {
            return;
        }
        let next = match self.search.current_match {
            Some(current) => (current + 1) % self.search.matches.len(),
            None => 0,
        };
        self.search.current_match = Some(next);
        self.jump_to_current_match(height);
    }

    pub(crate) fn jump_previous_match(&mut self, height: u16) {
        if self.search.matches.is_empty() {
            return;
        }
        let next = match self.search.current_match {
            Some(0) | None => self.search.matches.len().saturating_sub(1),
            Some(current) => current.saturating_sub(1),
        };
        self.search.current_match = Some(next);
        self.jump_to_current_match(height);
    }

    pub(crate) fn status_label(&self) -> String {
        let total = self.line_count();
        let line = (self.scroll_top + 1).min(total);
        let match_status = if self.search.query.is_empty() {
            "search off".to_string()
        } else if self.search.matches.is_empty() {
            format!("search '{}' (0 matches)", self.search.query)
        } else {
            let current = self.search.current_match.unwrap_or(0) + 1;
            format!("search '{}' ({}/{})", self.search.query, current, self.search.matches.len())
        };
        format!("line {line}/{total} • {match_status}")
    }

    fn refresh_messages(&mut self, session: &Session, width: u16) {
        let tool_output_blocks = &session.tool_output_blocks;
        let previous_len = self.messages.len();
        let current_len = tool_output_blocks.len();
        let width_changed = self.width != width;

        if current_len < previous_len {
            self.messages.truncate(current_len);
            self.cached_export_text = None;
        }
        while self.messages.len() < current_len {
            self.messages.push(CachedToolOutputBlock::default());
        }

        let first_dirty = if width_changed {
            0
        } else if current_len > previous_len {
            previous_len
        } else {
            current_len
        };

        for (index, block) in tool_output_blocks.iter().enumerate().skip(first_dirty) {
            if width_changed || self.messages[index].revision != session.tool_output_revision {
                self.messages[index] = CachedToolOutputBlock {
                    revision: session.tool_output_revision,
                    lines: collect_tool_output_lines(block, width),
                    lowered_lines: None,
                };
                self.cached_export_text = None;
            }
        }

        self.update_row_offsets_from(first_dirty.min(current_len));
    }

    fn update_row_offsets_from(&mut self, start_index: usize) {
        if start_index == 0 {
            self.row_offsets.clear();
            self.row_offsets.reserve(self.messages.len());
        } else {
            self.row_offsets.truncate(start_index);
        }

        let mut current_offset = self
            .row_offsets
            .last()
            .map(|offset| offset + self.messages[self.row_offsets.len() - 1].lines.len())
            .unwrap_or(0);

        for message in self.messages.iter().skip(self.row_offsets.len()) {
            self.row_offsets.push(current_offset);
            current_offset += message.lines.len();
        }

        self.total_lines = current_offset;
    }

    fn line_text_at(&self, row: usize) -> Option<&str> {
        if row >= self.total_lines {
            return None;
        }

        let mut message_index = self.row_offsets.partition_point(|offset| *offset <= row).saturating_sub(1);
        loop {
            let message = self.messages.get(message_index)?;
            let local_index = row.saturating_sub(self.row_offsets[message_index]);
            if let Some(line) = message.lines.get(local_index) {
                return Some(line.as_str());
            }
            if message_index == 0 {
                return None;
            }
            message_index -= 1;
        }
    }

    fn current_match_line(&self) -> Option<usize> {
        self.search
            .current_match
            .and_then(|index| self.search.matches.get(index).copied())
    }

    fn jump_to_current_match(&mut self, height: u16) {
        let Some(line) = self.current_match_line() else {
            return;
        };
        self.scroll_top = line.min(self.max_scroll(height));
    }

    fn recompute_matches(&mut self) {
        self.search.matches.clear();
        if self.search.query.is_empty() {
            self.search.current_match = None;
            return;
        }

        let needle = self.search.query.to_ascii_lowercase();
        let mut row_index = 0usize;
        for message in &mut self.messages {
            let lowered_lines = message
                .lowered_lines
                .get_or_insert_with(|| message.lines.iter().map(|line| line.to_ascii_lowercase()).collect());
            for line in lowered_lines {
                if line.contains(&needle) {
                    self.search.matches.push(row_index);
                }
                row_index += 1;
            }
        }

        if let Some(current) = self.search.current_match
            && current < self.search.matches.len()
        {
            return;
        }

        self.search.current_match = (!self.search.matches.is_empty()).then_some(0);
    }

    fn clamp_scroll(&mut self, height: u16) {
        self.scroll_top = self.scroll_top.min(self.max_scroll(height));
    }

    fn max_scroll(&self, height: u16) -> usize {
        self.total_lines.saturating_sub(usize::from(height.max(1)))
    }

    fn is_at_bottom(&self, height: u16) -> bool {
        self.scroll_top >= self.max_scroll(height)
    }

    fn page_step(height: u16) -> usize {
        usize::from(height.max(2)).saturating_sub(1)
    }
}

fn collect_tool_output_lines(block: &ToolOutputBlock, width: u16) -> Vec<String> {
    let max_width = usize::from(width.max(1));
    let mut lines = block
        .lines
        .iter()
        .flat_map(|line| wrap_output_line(line, max_width))
        .collect::<Vec<_>>();
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn wrap_output_line(line: &str, width: usize) -> Vec<String> {
    if line.is_empty() {
        return vec![String::new()];
    }

    let mut wrapped = Vec::new();
    let mut current = String::new();
    let mut current_width: usize = 0;
    for ch in line.chars() {
        let char_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if !current.is_empty() && current_width.saturating_add(char_width) > width {
            wrapped.push(std::mem::take(&mut current));
            current_width = 0;
        }
        current.push(ch);
        current_width = current_width.saturating_add(char_width);
    }
    if !current.is_empty() {
        wrapped.push(current);
    }
    wrapped
}

pub(crate) fn render_tool_output_viewer(
    session: &Session,
    frame: &mut Frame<'_>,
    area: Rect,
    state: &ToolOutputViewerState,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let title = Line::from(vec![
        Span::styled(" Tool Output Viewer ", session.core.section_title_style().add_modifier(Modifier::BOLD)),
        Span::raw(" "),
        Span::styled(state.status_label(), session.core.header_secondary_style()),
    ]);
    let block = Block::default().borders(Borders::ALL).title(title);
    frame.render_widget(Clear, area);
    frame.render_widget(block.clone(), area);
    let inner = block.inner(area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let show_search = state.search_active();
    let chunks = if show_search {
        Layout::vertical([Constraint::Min(1), Constraint::Length(2)]).split(inner)
    } else {
        Layout::vertical([Constraint::Min(1)]).split(inner)
    };
    let content_height = chunks[0].height;
    let lines = state.visible_lines(usize::from(content_height));
    frame.render_widget(Paragraph::new(lines).style(session.core.styles.default_style()), chunks[0]);

    if show_search && chunks.len() > 1 {
        let input_styles = input_styles_from_theme(&session.core.theme);
        let input_widget = Input::new("Search")
            .placeholder("type to search...")
            .prompt("/")
            .styles(input_styles);

        let mut input_state = InputState::new();
        let query = state.search_query().to_string();
        input_state.set_value(query.clone());
        input_state.set_focused(true);
        for _ in 0..query.chars().count() {
            input_state.move_right();
        }

        frame.render_stateful_widget(&input_widget, chunks[1], &mut input_state);
    }
}

pub(crate) fn viewer_content_width(area: Rect) -> u16 {
    area.width.saturating_sub(2).min(ui::TUI_MAX_VIEWPORT_WIDTH)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::core_tui::app::session::AppSession;
    use crate::tui::core_tui::types::InlineTheme;

    fn test_session() -> AppSession {
        AppSession::new(InlineTheme::default(), None, 24)
    }

    fn add_block(session: &mut AppSession, lines: &[&str]) {
        session.tool_output_blocks.push(ToolOutputBlock {
            lines: lines.iter().map(|line| (*line).to_string()).collect(),
        });
        session.tool_output_revision += 1;
    }

    #[test]
    fn refresh_appends_without_rebuilding_unchanged_blocks() {
        let mut session = test_session();
        add_block(&mut session, &["• Ran first", "  └ alpha"]);
        add_block(&mut session, &["• Ran second", "  └ beta"]);

        let mut viewer = ToolOutputViewerState::open(&session, 40, 10);
        let original_first = viewer.messages[0].revision;

        add_block(&mut session, &["• Ran third", "  └ gamma"]);
        viewer.refresh(&session, 40, 10);

        assert_eq!(viewer.messages[0].revision, original_first);
        assert_eq!(viewer.messages.len(), 3);
        assert!(viewer.export_text().contains("gamma"));
    }

    #[test]
    fn refresh_reflows_blocks_when_width_changes() {
        let mut session = test_session();
        add_block(&mut session, &["• Ran a command with a long output line"]);

        let mut viewer = ToolOutputViewerState::open(&session, 80, 10);
        let wide_lines = viewer.messages[0].lines.len();
        viewer.refresh(&session, 12, 10);

        assert!(viewer.messages[0].lines.len() > wide_lines);
    }

    #[test]
    fn search_uses_cached_lowercase_lines() {
        let mut session = test_session();
        add_block(&mut session, &["• Ran Alpha"]);
        add_block(&mut session, &["  └ beta alpha"]);

        let mut viewer = ToolOutputViewerState::open(&session, 40, 10);
        viewer.search.query = "alpha".to_string();
        viewer.recompute_matches();
        let lowered = viewer.messages[0].lowered_lines.as_ref().expect("lowered lines cached")[0].clone();

        viewer.jump_next_match(10);
        viewer.recompute_matches();

        assert!(lowered.contains("alpha"));
        assert_eq!(viewer.search.matches, vec![0, 1]);
    }

    #[test]
    fn export_text_is_cached_until_a_new_block_arrives() {
        let mut session = test_session();
        add_block(&mut session, &["• Ran alpha"]);

        let mut viewer = ToolOutputViewerState::open(&session, 40, 10);
        let exported = viewer.export_text();
        assert!(exported.contains("alpha"));
        assert_eq!(viewer.cached_export_text.as_deref(), Some(exported.as_str()));

        add_block(&mut session, &["• Ran beta"]);
        viewer.refresh(&session, 40, 10);

        assert_eq!(viewer.cached_export_text, None);
        let refreshed = viewer.export_text();
        assert!(refreshed.contains("alpha"));
        assert!(refreshed.contains("beta"));
    }

    #[test]
    fn viewer_keeps_complete_output_for_each_tool_call() {
        let mut session = test_session();
        add_block(
            &mut session,
            &[
                "• Ran cargo check",
                "  └ first complete line",
                "    final complete line",
            ],
        );
        add_block(&mut session, &["• Ran cargo fmt", "  └ fmt complete"]);

        let viewer = ToolOutputViewerState::open(&session, 80, 10);
        let export = viewer.clone().export_text();

        assert!(export.contains("first complete line"));
        assert!(export.contains("final complete line"));
        assert!(export.contains("• Ran cargo fmt"));
        assert!(!export.contains("Ran 2 commands"));
    }

    #[test]
    fn wrapping_preserves_blank_output_lines() {
        assert_eq!(wrap_output_line("", 20), vec![String::new()]);
        assert_eq!(wrap_output_line("abcdef", 3), vec!["abc", "def"]);
    }
}
