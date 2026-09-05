use vtcode_commons::ansi_capabilities::{ColorScheme, detect_color_scheme};

use crate::theme::color_math::relative_luminance;
use crate::theme::registry::{available_theme_suites, theme_definition, theme_suite_id};
use crate::theme::types::DEFAULT_THEME_ID;

/// Report whether a theme matches the detected terminal light/dark scheme.
pub fn theme_matches_terminal_scheme(theme_id: &str) -> bool {
    let scheme = detect_color_scheme();
    let theme_is_light = is_light_theme(theme_id);

    match scheme {
        ColorScheme::Light => theme_is_light,
        ColorScheme::Dark | ColorScheme::Unknown => !theme_is_light,
    }
}

/// Report whether a built-in theme should be treated as a light theme.
pub fn is_light_theme(theme_id: &str) -> bool {
    theme_definition(theme_id)
        .map(|theme| relative_luminance(theme.palette.background) > 0.5)
        .unwrap_or(false)
}

/// Suggest a built-in theme that matches the current terminal scheme.
pub fn suggest_theme_for_terminal() -> &'static str {
    match detect_color_scheme() {
        ColorScheme::Light => "vitesse-light",
        ColorScheme::Dark | ColorScheme::Unknown => DEFAULT_THEME_ID,
    }
}

/// Pick the theme to apply after a terminal light/dark scheme change.
///
/// Prefers a light/dark twin from the same suite as `current_theme_id` so
/// suite-aware themes stay within their family; falls back to
/// [`suggest_theme_for_terminal`] for single-theme or custom selections.
pub fn theme_for_terminal_scheme_change(current_theme_id: &str, dark: bool) -> &'static str {
    if let Some(suite_id) = theme_suite_id(current_theme_id)
        && let Some(suite) = available_theme_suites().into_iter().find(|suite| suite.id == suite_id)
    {
        for theme_id in suite.theme_ids {
            if is_light_theme(theme_id) != dark {
                return theme_id;
            }
        }
    }
    suggest_theme_for_terminal()
}
