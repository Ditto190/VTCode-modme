#![cfg(unix)]
#![allow(
    missing_docs,
    reason = "Intentional compatibility, platform, or test-only suppression."
)]
use vtcode_commons::ansi_capabilities::{ColorScheme, set_color_scheme_override};

use super::super::*;
use super::helpers::*;

#[test]
fn color_scheme_report_ignored_when_auto_disabled() {
    set_color_scheme_override(Some(ColorScheme::Dark));
    crate::theme::set_active_theme("ciapre").expect("ciapre is a built-in theme");
    let mut session = Session::new(InlineTheme::default(), None, VIEW_ROWS);

    session.apply_terminal_color_scheme_report(false);

    assert_eq!(crate::theme::active_theme_id(), "ciapre", "report must be ignored without the auto opt-in");
}

#[test]
fn color_scheme_report_switches_to_matching_theme_when_auto_enabled() {
    set_color_scheme_override(Some(ColorScheme::Dark));
    crate::theme::set_active_theme("ciapre").expect("ciapre is a built-in theme");
    let mut session = Session::new(InlineTheme::default(), None, VIEW_ROWS);
    session.handle_command(InlineCommand::SetColorSchemeAuto { enabled: true });

    session.apply_terminal_color_scheme_report(false);

    let active = crate::theme::active_theme_id();
    assert!(
        crate::theme::is_light_theme(&active),
        "light report must switch the dark theme to a light one, got {active}"
    );
}

#[test]
fn color_scheme_report_matching_scheme_is_noop() {
    set_color_scheme_override(Some(ColorScheme::Dark));
    crate::theme::set_active_theme("ciapre").expect("ciapre is a built-in theme");
    let mut session = Session::new(InlineTheme::default(), None, VIEW_ROWS);
    session.handle_command(InlineCommand::SetColorSchemeAuto { enabled: true });

    session.apply_terminal_color_scheme_report(true);

    assert_eq!(
        crate::theme::active_theme_id(),
        "ciapre",
        "dark report while a dark theme is active must not retint"
    );
}
