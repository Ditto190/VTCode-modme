#![allow(
    missing_docs,
    reason = "Intentional compatibility, platform, or test-only suppression."
)]
use super::super::*;
use super::helpers::*;

#[test]
fn diff_overlay_defaults_to_edit_approval_mode() {
    let preview = app_types::DiffPreviewState::new(
        "src/main.rs".to_string(),
        "before".to_string(),
        "after".to_string(),
        Vec::new(),
    );

    assert_eq!(preview.mode, app_types::DiffPreviewMode::EditApproval);
    assert!(preview.current_hunk_ref().is_some());
}

#[test]
fn diff_overlay_renders_side_by_side_when_configured() {
    use vtcode_commons::ui_protocol::DiffPreviewMode as DiffLayoutMode;

    let mut session = AppSession::new(InlineTheme::default(), None, VIEW_ROWS);
    let mut appearance = session.core.appearance.clone();
    appearance.diff_preview_mode = DiffLayoutMode::SideBySide;
    session.handle_command(app_types::InlineCommand::SetAppearance { appearance });

    show_diff_overlay(&mut session, app_types::DiffPreviewMode::EditApproval);
    let lines = rendered_app_session_lines(&mut session, VIEW_ROWS);
    let joined = lines.join("\n");

    assert!(joined.contains("side-by-side"), "header should mark side-by-side layout");
    assert!(joined.contains("Old"), "left column header should appear");
    assert!(joined.contains("New"), "right column header should appear");
}

#[test]
fn diff_overlay_renders_inline_by_default() {
    let mut session = AppSession::new(InlineTheme::default(), None, VIEW_ROWS);
    show_diff_overlay(&mut session, app_types::DiffPreviewMode::EditApproval);
    let lines = rendered_app_session_lines(&mut session, VIEW_ROWS);
    let joined = lines.join("\n");

    assert!(!joined.contains("side-by-side"), "inline layout should not show the side-by-side badge");
}

#[test]
fn diff_overlay_header_keeps_counts_visible_for_long_paths() {
    let mut session = AppSession::new(InlineTheme::default(), None, VIEW_ROWS);
    let long_path = format!("src/{}", "very/long/directory/segment/".repeat(12) + "component.rs");
    session.show_diff_overlay(app_types::DiffOverlayRequest {
        file_path: long_path,
        before: "fn old() {}\n".to_string(),
        after: "fn new() {}\n".to_string(),
        hunks: Vec::new(),
        current_hunk: 0,
        mode: app_types::DiffPreviewMode::EditApproval,
        unified: None,
    });

    let lines = rendered_app_session_lines(&mut session, VIEW_ROWS);
    let header_line = lines
        .iter()
        .find(|line| line.contains("← Edit"))
        .expect("header line should render");
    assert!(header_line.contains("(+1 -1)"), "counts must stay visible: {header_line}");
}

#[test]
fn diff_overlay_edit_approval_keys_remain_unchanged() {
    let mut session = AppSession::new(InlineTheme::default(), None, VIEW_ROWS);

    show_diff_overlay(&mut session, app_types::DiffPreviewMode::EditApproval);
    let apply = session.process_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(
        apply,
        Some(app_types::InlineEvent::Transient(app_types::TransientEvent::Submitted(
            app_types::TransientSubmission::DiffApply
        )))
    ));

    show_diff_overlay(&mut session, app_types::DiffPreviewMode::EditApproval);
    let reload = session.process_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE));
    assert!(reload.is_none());
    assert!(session.diff_preview_state().is_some());

    let reject = session.process_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(matches!(
        reject,
        Some(app_types::InlineEvent::Transient(app_types::TransientEvent::Submitted(
            app_types::TransientSubmission::DiffReject
        )))
    ));
}

#[test]
fn diff_overlay_scrolls_and_hunk_navigation_updates_cached_position() {
    let mut session = AppSession::new(InlineTheme::default(), None, VIEW_ROWS);
    let before = (0..20).map(|index| format!("old-{index}\n")).collect::<String>();
    let mut after_lines = (0..20).map(|index| format!("old-{index}\n")).collect::<Vec<_>>();
    after_lines[1] = "new-1\n".to_string();
    after_lines[18] = "new-18\n".to_string();
    session.show_diff_overlay(app_types::DiffOverlayRequest {
        file_path: "src/main.rs".to_string(),
        before,
        after: after_lines.concat(),
        hunks: Vec::new(),
        current_hunk: 0,
        mode: app_types::DiffPreviewMode::ReadonlyReview,
        unified: None,
    });

    let down = session.process_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    assert!(down.is_none());
    assert_eq!(session.diff_preview_state().map(|state| state.scroll_offset), Some(1));

    let tab = session.process_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    assert!(tab.is_none());
    let state = session.diff_preview_state().expect("preview remains open");
    assert_eq!(state.current_hunk, 1);
    assert!(state.scroll_offset > 1, "second hunk should move the cached viewport");

    let back = session.process_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
    assert!(back.is_none());
    let state = session.diff_preview_state().expect("preview remains open");
    assert_eq!(state.current_hunk, 0);
    assert_eq!(state.scroll_offset, 0);
}

#[test]
fn diff_overlay_keeps_large_preview_visible_after_scrolling() {
    let mut session = AppSession::new(InlineTheme::default(), None, VIEW_ROWS);
    let before = (0..2_100).map(|index| format!("old-{index}\n")).collect::<String>();
    let after = (0..2_100).map(|index| format!("new-{index}\n")).collect::<String>();
    session.show_diff_overlay(app_types::DiffOverlayRequest {
        file_path: "src/main.rs".to_string(),
        before,
        after,
        hunks: Vec::new(),
        current_hunk: 0,
        mode: app_types::DiffPreviewMode::ReadonlyReview,
        unified: None,
    });

    for index in 0..200 {
        let event = session.process_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE));
        assert!(event.is_none(), "unexpected event at {index}: {event:?}");
    }

    let lines = rendered_app_session_lines(&mut session, VIEW_ROWS);
    assert!(!lines.iter().any(|line| line.contains("(no changes)")));
}

#[test]
fn diff_overlay_conflict_mode_maps_enter_reload_and_escape() {
    let mut session = AppSession::new(InlineTheme::default(), None, VIEW_ROWS);

    show_diff_overlay(&mut session, app_types::DiffPreviewMode::FileConflict);
    let proceed = session.process_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(
        proceed,
        Some(app_types::InlineEvent::Transient(app_types::TransientEvent::Submitted(
            app_types::TransientSubmission::DiffProceed
        )))
    ));

    show_diff_overlay(&mut session, app_types::DiffPreviewMode::FileConflict);
    let reload = session.process_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE));
    assert!(matches!(
        reload,
        Some(app_types::InlineEvent::Transient(app_types::TransientEvent::Submitted(
            app_types::TransientSubmission::DiffReload
        )))
    ));

    show_diff_overlay(&mut session, app_types::DiffPreviewMode::FileConflict);
    let abort = session.process_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(matches!(
        abort,
        Some(app_types::InlineEvent::Transient(app_types::TransientEvent::Submitted(
            app_types::TransientSubmission::DiffAbort
        )))
    ));
}

#[test]
fn diff_overlay_conflict_mode_ignores_trust_shortcuts() {
    let mut session = AppSession::new(InlineTheme::default(), None, VIEW_ROWS);

    show_diff_overlay(&mut session, app_types::DiffPreviewMode::FileConflict);
    let event = session.process_key(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE));

    assert!(event.is_none());
    assert!(matches!(
        session.diff_preview_state().map(|preview| preview.mode),
        Some(app_types::DiffPreviewMode::FileConflict)
    ));
}

#[test]
fn diff_overlay_readonly_review_maps_enter_and_escape_to_back() {
    let mut session = AppSession::new(InlineTheme::default(), None, VIEW_ROWS);

    show_diff_overlay(&mut session, app_types::DiffPreviewMode::ReadonlyReview);
    let enter = session.process_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(
        enter,
        Some(app_types::InlineEvent::Transient(app_types::TransientEvent::Submitted(
            app_types::TransientSubmission::DiffAbort
        )))
    ));

    show_diff_overlay(&mut session, app_types::DiffPreviewMode::ReadonlyReview);
    let escape = session.process_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(matches!(
        escape,
        Some(app_types::InlineEvent::Transient(app_types::TransientEvent::Submitted(
            app_types::TransientSubmission::DiffAbort
        )))
    ));
}

#[test]
fn diff_overlay_readonly_review_ignores_reload_shortcut() {
    let mut session = AppSession::new(InlineTheme::default(), None, VIEW_ROWS);

    show_diff_overlay(&mut session, app_types::DiffPreviewMode::ReadonlyReview);
    let reload = session.process_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE));

    assert!(reload.is_none());
    assert!(matches!(
        session.diff_preview_state().map(|preview| preview.mode),
        Some(app_types::DiffPreviewMode::ReadonlyReview)
    ));
}

#[test]
fn diff_overlay_opens_from_unified_preview_for_completed_edit_review() {
    let mut session = AppSession::new(InlineTheme::default(), None, VIEW_ROWS);
    let unified = "@@ -1,3 +1,3 @@\n fn main() {\n-    let old = 1;\n+    let new = 2;\n }\n";
    session.show_diff_overlay(app_types::DiffOverlayRequest {
        file_path: "src/main.rs".to_string(),
        before: String::new(),
        after: String::new(),
        hunks: Vec::new(),
        current_hunk: 0,
        mode: app_types::DiffPreviewMode::ReadonlyReview,
        unified: Some(unified.to_string()),
    });

    let state = session.diff_preview_state().expect("unified review overlay opens");
    assert_eq!(state.mode, app_types::DiffPreviewMode::ReadonlyReview);
    assert!(
        state.display_lines.iter().any(|line| line.text.contains("let new = 2")),
        "unified body must populate display lines: {:?}",
        state.display_lines
    );

    let lines = rendered_app_session_lines(&mut session, VIEW_ROWS);
    let joined = lines.join("\n");
    assert!(joined.contains("← Review"), "full-viewport review header should render");
    assert!(joined.contains("let new = 2") || joined.contains("+"), "wrapped review body should be visible");
}

#[test]
fn diff_preview_suspends_task_panel_and_restores_it_on_close() {
    let mut session = AppSession::new(InlineTheme::default(), None, 30);
    session.task_panel_lines = vec!["Queued task".to_string()];
    session.set_task_panel_visible(true);

    let backend = TestBackend::new(VIEW_WIDTH, 30);
    let mut terminal = Terminal::new(backend).expect("failed to create test terminal");
    terminal
        .draw(|frame| session.render(frame))
        .expect("failed to render task panel");
    assert!(session.core.bottom_panel_area().is_some());

    show_diff_overlay(&mut session, app_types::DiffPreviewMode::ReadonlyReview);
    assert!(!session.core.input_enabled());
    assert!(!session.core.build_input_widget_data(VIEW_WIDTH, 1).cursor_should_be_visible);

    terminal
        .draw(|frame| session.render(frame))
        .expect("failed to render diff preview");
    assert!(
        session.core.bottom_panel_area().is_none(),
        "floating diff preview should hide the lower bottom panel"
    );

    session.close_diff_overlay();
    assert!(session.core.input_enabled());
    assert!(session.core.build_input_widget_data(VIEW_WIDTH, 1).cursor_should_be_visible);

    let lines = rendered_app_session_lines(&mut session, 30);
    assert!(
        lines.iter().any(|line| line.contains("Queued task")),
        "task panel should resume after closing diff preview"
    );
}

#[test]
fn diff_overlay_resize_preserves_the_transcript_scroll_anchor() {
    let mut session = AppSession::new(InlineTheme::default(), None, 30);
    for index in 0..80 {
        session.core.push_line(
            InlineMessageKind::Agent,
            vec![make_segment(&format!(
                "history-{index} with enough text to wrap differently when the terminal becomes narrow"
            ))],
        );
    }

    let backend = TestBackend::new(80, 30);
    let mut terminal = Terminal::new(backend).expect("wide test terminal");
    terminal.draw(|frame| session.render(frame)).expect("render wide transcript");
    session.core.scroll_page_up();
    session.core.scroll_page_up();
    let before = session.core.transcript_scroll_anchor().expect("history anchor before overlay");

    show_diff_overlay(&mut session, app_types::DiffPreviewMode::ReadonlyReview);
    let backend = TestBackend::new(56, 20);
    let mut terminal = Terminal::new(backend).expect("narrow test terminal");
    terminal
        .draw(|frame| session.render(frame))
        .expect("render narrow diff overlay");
    session.close_diff_overlay();

    let backend = TestBackend::new(80, 30);
    let mut terminal = Terminal::new(backend).expect("restored test terminal");
    terminal
        .draw(|frame| session.render(frame))
        .expect("render restored transcript");
    let after = session.core.transcript_scroll_anchor().expect("history anchor after overlay");

    assert_eq!(after, before);
}
