#![allow(
    missing_docs,
    reason = "Intentional compatibility, platform, or test-only suppression."
)]
use super::super::*;
use super::helpers::*;

fn push_agent_lines(session: &mut Session, count: usize) {
    for index in 0..count {
        session.push_line(InlineMessageKind::Agent, vec![make_segment(&format!("line {index}"))]);
    }
}

fn prepare_sized_session(line_count: usize) -> Session {
    let mut session = Session::new(InlineTheme::default(), None, VIEW_ROWS);
    session.apply_transcript_width(VIEW_WIDTH);
    session.apply_transcript_rows(6);
    push_agent_lines(&mut session, line_count);
    session.ensure_scroll_metrics();
    session
}

#[test]
fn distinct_changes_tracked_but_streaming_repeats_collapse() {
    let mut session = prepare_sized_session(0);
    session.push_line(InlineMessageKind::Agent, vec![make_segment("a")]);
    session.push_line(InlineMessageKind::Agent, vec![make_segment("b")]);
    session.push_line(InlineMessageKind::Agent, vec![make_segment("c")]);
    assert_eq!(session.recent_change_line_idxs.len(), 3);
    assert_eq!(session.last_change_line_idx, Some(2));

    // Streaming chunks to the same tail line must not grow distinct history.
    session.append_inline(InlineMessageKind::Agent, make_segment(" +more"));
    session.append_inline(InlineMessageKind::Agent, make_segment(" +even-more"));
    assert_eq!(session.last_change_line_idx, Some(2));
    assert_eq!(session.recent_change_line_idxs.len(), 3, "consecutive repeats for one index collapse");
}

#[test]
fn gate_needs_scrolled_up_and_two_changes_asymmetric() {
    let mut one_change = prepare_sized_session(10);
    // Collapse history to a single distinct change while keeping scrollable content.
    one_change.recent_change_line_idxs.clear();
    one_change.recent_change_line_idxs.push_back(9);
    one_change.last_change_line_idx = Some(9);
    one_change.scroll_page_up();
    assert!(!one_change.should_show_jump_to_last_change(), "single change must not show even while scrolled");

    let mut two_at_bottom = prepare_sized_session(10);
    assert!(!two_at_bottom.should_show_jump_to_last_change(), "at live bottom there is nothing to jump to");

    two_at_bottom.scroll_page_up();
    assert!(two_at_bottom.should_show_jump_to_last_change(), "scrolled + >=2 distinct changes shows");
}

#[test]
fn jump_pins_tail_change_to_bottom_with_sticky_highlight() {
    let mut session = prepare_sized_session(10);
    session.scroll_page_up();
    assert!(session.scroll_offset() > 0);
    assert!(session.should_show_jump_to_last_change());

    assert!(session.jump_to_last_change());
    assert_eq!(session.scroll_offset(), 0, "tail change pins to live bottom (offset 0)");
    assert_eq!(session.jump_highlight_line_idx, Some(9));
    assert!(!session.should_show_jump_to_last_change(), "at bottom the pill hides");

    // Sticky: highlight survives no-op renders but clears on manual scroll.
    let highlight = session.jump_highlight_line_idx;
    let _ = visible_transcript(&mut session);
    assert_eq!(session.jump_highlight_line_idx, highlight);
    session.scroll_line_up();
    assert_eq!(session.jump_highlight_line_idx, None);
}

#[test]
fn jump_pins_earlier_change_above_bottom_asymmetric() {
    let mut session = prepare_sized_session(10);
    // Simulate an edit to an earlier line after newer lines exist.
    session.record_transcript_change(1);
    session.scroll_page_up();
    let max_offset = session.current_max_scroll_offset();
    assert!(max_offset > 0);

    assert!(session.jump_to_last_change());
    let offset_after_early = session.scroll_offset();
    assert!(offset_after_early > 0, "earlier change stays scrolled (offset {offset_after_early})");
    assert_eq!(session.jump_highlight_line_idx, Some(1));

    // Contrast: tail change jumps to zero, earlier change does not.
    session.record_transcript_change(9);
    session.scroll_page_up();
    assert!(session.jump_to_last_change());
    assert_eq!(session.scroll_offset(), 0);
}

#[test]
fn new_distinct_change_clears_sticky_highlight() {
    let mut session = prepare_sized_session(5);
    session.scroll_page_up();
    assert!(session.jump_to_last_change());
    assert_eq!(session.jump_highlight_line_idx, Some(4));

    session.push_line(InlineMessageKind::Agent, vec![make_segment("new tail")]);
    assert_eq!(session.jump_highlight_line_idx, None);
    assert_eq!(session.last_change_line_idx, Some(5));
}

#[test]
fn clear_screen_resets_jump_state() {
    let mut session = prepare_sized_session(4);
    session.scroll_page_up();
    assert!(session.jump_to_last_change());
    session.clear_screen();
    assert_eq!(session.last_change_line_idx, None);
    assert!(session.recent_change_line_idxs.is_empty());
    assert_eq!(session.jump_highlight_line_idx, None);
    assert!(!session.should_show_jump_to_last_change());
}

#[test]
fn eviction_shifts_tracked_indices_and_drops_prefix() {
    let mut session = prepare_sized_session(5);
    // History is [0,1,2,3,4], last=4.
    session.shift_tracked_changes_after_eviction(2);
    assert_eq!(session.last_change_line_idx, Some(2));
    assert_eq!(session.recent_change_line_idxs.iter().copied().collect::<Vec<_>>(), vec![0, 1, 2]);

    // Evicting past the last change clears it instead of underflowing.
    session.shift_tracked_changes_after_eviction(5);
    assert_eq!(session.last_change_line_idx, None);
    assert!(session.recent_change_line_idxs.is_empty());
}

#[test]
fn jump_returns_false_without_target_or_width() {
    let mut empty = Session::new(InlineTheme::default(), None, VIEW_ROWS);
    assert!(!empty.jump_to_last_change());

    let mut no_width = Session::new(InlineTheme::default(), None, VIEW_ROWS);
    no_width.push_line(InlineMessageKind::Agent, vec![make_segment("x")]);
    no_width.push_line(InlineMessageKind::Agent, vec![make_segment("y")]);
    // Width 0 blocks row mapping.
    assert!(!no_width.jump_to_last_change());
}

#[test]
fn default_binding_is_ctrl_end_with_label() {
    use crate::tui::core_tui::session::action::BindingStore;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let store = BindingStore::default();
    let key = KeyEvent::new(KeyCode::End, KeyModifiers::CONTROL);
    assert_eq!(store.resolve(&key), Some(Action::JumpToLastChange));
    assert!(store.primary_key_label(Action::JumpToLastChange).is_some());
}

#[test]
fn jump_dispatch_falls_back_to_bottom_when_scrolled_without_target() {
    let mut session = prepare_sized_session(8);
    session.scroll_page_up();
    assert!(session.scroll_offset() > 0);
    // Simulate fresh tracking (e.g. after clear + rescroll with no changes).
    session.last_change_line_idx = None;
    session.recent_change_line_idxs.clear();

    let event = events::dispatch_rebindable_action(&mut session, Action::JumpToLastChange);
    assert!(matches!(event, Some(InlineEvent::JumpToLastChange)));
    assert_eq!(session.scroll_offset(), 0);
}

#[test]
fn pill_rect_only_when_gated_and_inside_transcript() {
    let mut session = prepare_sized_session(10);
    // No transcript area before first render.
    assert_eq!(session.jump_pill_rect(), None);

    let _ = visible_transcript(&mut session);
    // At bottom the pill hides.
    assert_eq!(session.jump_pill_rect(), None);

    session.scroll_page_up();
    let area = session.transcript_area().expect("render sets transcript area");
    let pill = session.jump_pill_rect().expect("scrolled + >=2 shows pill");
    assert_eq!(pill.height, 1);
    assert_eq!(pill.y, area.bottom().saturating_sub(1));
    assert!(pill.x >= area.x);
    assert!(pill.right() <= area.right());
}

#[test]
fn middle_removal_clears_stale_tracking_when_empty_and_dedupes() {
    let mut session = prepare_sized_session(3);
    // History [0,1,2]; shifting for middle removal 1 collapses 2->1.
    session.shift_tracked_changes_after_removal(1);
    assert_eq!(
        session.recent_change_line_idxs.iter().copied().collect::<Vec<_>>(),
        vec![0, 1],
        "entries collapsing onto the removal point dedupe"
    );

    // Force a duplicate-prone shape then remove: history [1,2] removing 1.
    session.recent_change_line_idxs.clear();
    session.recent_change_line_idxs.push_back(1);
    session.recent_change_line_idxs.push_back(2);
    session.shift_tracked_changes_after_removal(1);
    assert_eq!(
        session.recent_change_line_idxs.iter().copied().collect::<Vec<_>>(),
        vec![1],
        "entries collapsing onto the removal point dedupe"
    );

    // Empty transcript clears instead of leaving stale indices.
    session.lines.clear();
    session.shift_tracked_changes_after_removal(0);
    assert_eq!(session.last_change_line_idx, None);
    assert!(session.recent_change_line_idxs.is_empty());
    assert_eq!(session.jump_highlight_line_idx, None);
    assert!(!session.should_show_jump_to_last_change());
}

#[test]
fn footer_rect_only_when_gated_and_inside_status() {
    let mut session = prepare_sized_session(10);
    // No status area before a full frame render.
    assert_eq!(session.footer_jump_rect(), None);

    let _ = rendered_session_lines(&mut session, VIEW_ROWS);
    // At the live bottom edge the footer affordance hides.
    assert_eq!(session.footer_jump_rect(), None);

    session.scroll_page_up();
    let _ = rendered_session_lines(&mut session, VIEW_ROWS);
    let status = session.input_status_area().expect("full render sets status area");
    let footer = session.footer_jump_rect().expect("scrolled + >=2 shows footer affordance");
    assert_eq!(footer.height, 1);
    assert_eq!(footer.y, status.y);
    assert!(footer.x >= status.x);
    assert!(footer.right() <= status.right());
    assert!(session.footer_jump_contains(footer.x, footer.y));
    // Left edge of the status row is outside the jump affordance.
    assert!(!session.footer_jump_contains(status.x, status.y));
}

#[test]
fn footer_click_jumps_to_last_change() {
    use ratatui::crossterm::event::KeyModifiers;

    let mut session = prepare_sized_session(10);
    session.scroll_page_up();
    let _ = rendered_session_lines(&mut session, VIEW_ROWS);
    let footer = session.footer_jump_rect().expect("footer affordance must be visible");
    assert!(session.scroll_offset() > 0);

    let (tx, mut rx) = mpsc::unbounded_channel();
    left_click_session(&mut session, &tx, footer.x, footer.y, KeyModifiers::NONE);
    assert_eq!(session.scroll_offset(), 0, "footer click pins to live bottom");
    assert_eq!(session.jump_highlight_line_idx, Some(9));
    assert!(matches!(rx.try_recv(), Ok(InlineEvent::JumpToLastChange)));
}

#[test]
fn footer_click_outside_jump_text_does_not_jump() {
    use ratatui::crossterm::event::KeyModifiers;

    let mut session = prepare_sized_session(10);
    session.scroll_page_up();
    let _ = rendered_session_lines(&mut session, VIEW_ROWS);
    let status = session.input_status_area().expect("status area");
    assert!(session.footer_jump_rect().is_some());
    let offset_before = session.scroll_offset();
    assert!(offset_before > 0);

    let (tx, _rx) = mpsc::unbounded_channel();
    left_click_session(&mut session, &tx, status.x, status.y, KeyModifiers::NONE);
    assert_eq!(session.jump_highlight_line_idx, None, "left status click must not arm highlight");
    assert_eq!(session.scroll_offset(), offset_before, "offset must not snap to bottom");
}
