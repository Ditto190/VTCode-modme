#![allow(missing_docs)]
use super::super::*;
use crate::tui::prelude::InlineSegment;
use std::sync::Arc;
use vtcode_commons::ui_protocol::ThinkingBlockState;

fn make_policy_line(text: &str) -> InlineSegment {
    InlineSegment {
        text: text.to_string(),
        style: Arc::new(InlineTextStyle::default()),
    }
}

fn push_policy_lines(session: &mut Session, texts: &[&str]) {
    for text in texts {
        session.push_line(InlineMessageKind::Policy, vec![make_policy_line(text)]);
    }
}

fn line_text(rendered: &TranscriptLine) -> String {
    rendered
        .line
        .spans
        .iter()
        .map(|span| span.content.to_string())
        .collect::<String>()
}

fn all_text(transcript: &[TranscriptLine]) -> String {
    transcript
        .iter()
        .map(line_text)
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn collapsed_by_default_renders_summary_line() {
    let session = Session::new(InlineTheme::default(), None, 24);
    let mut session = session;
    push_policy_lines(&mut session, &["reasoning step one", "reasoning step two"]);

    let start = session.lines.len() - 2;
    let transcript = session.reflow_message_lines(start, 100, true);
    let joined = all_text(&transcript);

    assert!(
        joined.contains("Thinking"),
        "collapsed summary should mention Thinking, got: {joined:?}"
    );
    assert!(
        !joined.contains("reasoning step one"),
        "collapsed render must not include the body, got: {joined:?}"
    );
}

#[test]
fn extended_config_renders_full_body() {
    let mut session = Session::new(InlineTheme::default(), None, 24);
    session.appearance.thinking_display = ThinkingBlockState::Extended;
    push_policy_lines(&mut session, &["reasoning step one", "reasoning step two"]);

    let start = session.lines.len() - 2;
    let transcript = session.reflow_message_lines(start, 100, true);
    let joined = all_text(&transcript);

    assert!(
        joined.contains("reasoning step one"),
        "extended render should include the body, got: {joined:?}"
    );
    assert!(
        !joined.contains("Thinking ("),
        "extended render must not show the collapsed summary, got: {joined:?}"
    );
}

#[test]
fn toggle_flips_collapse_state() {
    let mut session = Session::new(InlineTheme::default(), None, 24);
    session.transcript_width = 100;
    push_policy_lines(&mut session, &["reasoning step one", "reasoning step two"]);
    let start = session.lines.len() - 2;

    // Default is collapsed.
    let collapsed = session.reflow_message_lines(start, 100, true);
    assert!(all_text(&collapsed).contains("Thinking"));

    // Locate the summary row via the reflow cache.
    let summary_row = {
        let cache = session.ensure_reflow_cache(100);
        cache.row_offsets[start]
    };

    let toggled = session.toggle_thinking_block_at_row(100, summary_row);
    assert!(toggled, "toggle should report a toggled block");

    // Now expanded.
    let expanded = session.reflow_message_lines(start, 100, true);
    assert!(
        all_text(&expanded).contains("reasoning step one"),
        "after toggle the body should be visible"
    );

    // Toggle back to collapsed.
    let toggled_again = session.toggle_thinking_block_at_row(100, summary_row);
    assert!(toggled_again);
    let collapsed_again = session.reflow_message_lines(start, 100, true);
    assert!(all_text(&collapsed_again).contains("Thinking"));
}
