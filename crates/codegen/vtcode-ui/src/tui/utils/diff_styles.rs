//! Unified diff styles for TUI rendering
//!
//! Re-exports diff theme from vtcode-commons and provides
//! ratatui-specific style helpers for diff rendering.

// Re-export diff theme from vtcode-commons
pub use vtcode_commons::diff_theme::{
    DiffColorLevel, DiffTheme, diff_add_bg, diff_add_word_bg, diff_del_bg, diff_del_word_bg, diff_gutter_bg_add_light,
    diff_gutter_bg_del_light, diff_gutter_fg_light, diff_hunk_bg,
};
pub use vtcode_commons::styling::DiffColorPalette;

use crate::tui::ui::syntax_highlight::{DiffScopeBackgroundRgbs, diff_scope_background_rgbs};
use ratatui::style::{Color as RatatuiColor, Modifier, Style as RatatuiStyle};
use vtcode_commons::color256_theme::rgb_to_ansi256_for_theme;

use anstyle::{Color as AnstyleColor, Style as AnstyleStyle};

// ── WCAG AA accessible colours ─────────────────────────────────────────────
//
// Verified to pass 4.5:1 minimum contrast ratio on their respective tinted
// diff backgrounds (dark add bg #143A2D, dark del bg #46262A).

/// Foreground colour for deletion markers and content on dark themes.
/// Brighter than standard ANSI Red (#CD0000) to pass WCAG AA on dark del bg.
const DELETION_FG_DARK: RatatuiColor = RatatuiColor::Rgb(255, 90, 90);

/// Foreground colour for insertion markers and content on dark themes.
const INSERTION_FG_DARK: RatatuiColor = RatatuiColor::LightGreen;

/// Foreground colour for deletion markers on light themes.
const DELETION_FG_LIGHT: RatatuiColor = RatatuiColor::LightRed;

/// Foreground colour for insertion markers on light themes.
const INSERTION_FG_LIGHT: RatatuiColor = RatatuiColor::LightGreen;

// ── Conversion helpers ─────────────────────────────────────────────────────

/// Convert anstyle Color to ratatui Color.
fn ratatui_color_from_anstyle(color: anstyle::Color) -> RatatuiColor {
    crate::design::color::anstyle_to_ratatui_color(color)
}

// ── TUI-specific diff line styling ─────────────────────────────────────────

/// Diff line type for style selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffLineType {
    Insert,
    Delete,
    Context,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[allow(dead_code, reason = "foreground-only retains resolver for compatibility")]
struct ResolvedDiffBackgrounds {
    add: Option<RatatuiColor>,
    del: Option<RatatuiColor>,
}

/// Snapshot of diff styling inputs that can be reused while rendering.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiffRenderStyleContext {
    theme: DiffTheme,
    level: DiffColorLevel,
    #[allow(dead_code, reason = "foreground-only retains resolver for compatibility")]
    backgrounds: ResolvedDiffBackgrounds,
}

impl DiffRenderStyleContext {
    pub fn theme(self) -> DiffTheme {
        self.theme
    }

    pub fn level(self) -> DiffColorLevel {
        self.level
    }
}

/// Resolve the current terminal and syntax-theme styling into one context.
pub(crate) fn current_diff_render_style_context() -> DiffRenderStyleContext {
    let theme = DiffTheme::detect();
    let level = DiffColorLevel::detect();
    diff_render_style_context_for(theme, level, scope_backgrounds_for_level(level))
}

pub(crate) fn diff_render_style_context_for(
    theme: DiffTheme,
    level: DiffColorLevel,
    scope_backgrounds: DiffScopeBackgroundRgbs,
) -> DiffRenderStyleContext {
    DiffRenderStyleContext {
        theme,
        level,
        backgrounds: resolve_diff_backgrounds_for(theme, level, scope_backgrounds),
    }
}

fn resolve_diff_backgrounds_for(
    theme: DiffTheme,
    level: DiffColorLevel,
    scope_backgrounds: DiffScopeBackgroundRgbs,
) -> ResolvedDiffBackgrounds {
    let mut resolved = fallback_diff_backgrounds(theme, level);
    if level == DiffColorLevel::Ansi16 {
        return resolved;
    }

    if let Some(rgb) = scope_backgrounds.inserted
        && let Some(color) = color_from_rgb_for_level(rgb, theme, level)
    {
        resolved.add = Some(color);
    }

    if let Some(rgb) = scope_backgrounds.deleted
        && let Some(color) = color_from_rgb_for_level(rgb, theme, level)
    {
        resolved.del = Some(color);
    }

    resolved
}

fn fallback_diff_backgrounds(theme: DiffTheme, level: DiffColorLevel) -> ResolvedDiffBackgrounds {
    match level {
        DiffColorLevel::Ansi16 => ResolvedDiffBackgrounds::default(),
        DiffColorLevel::TrueColor | DiffColorLevel::Ansi256 => ResolvedDiffBackgrounds {
            add: Some(ratatui_color_from_anstyle(diff_add_bg(theme, level))),
            del: Some(ratatui_color_from_anstyle(diff_del_bg(theme, level))),
        },
    }
}

fn color_from_rgb_for_level(rgb: (u8, u8, u8), theme: DiffTheme, level: DiffColorLevel) -> Option<RatatuiColor> {
    match level {
        DiffColorLevel::TrueColor => Some(RatatuiColor::Rgb(rgb.0, rgb.1, rgb.2)),
        DiffColorLevel::Ansi256 => {
            Some(RatatuiColor::Indexed(rgb_to_ansi256_for_theme(rgb.0, rgb.1, rgb.2, theme.is_light())))
        }
        DiffColorLevel::Ansi16 => None,
    }
}

/// Full-width line background style. Foreground-only: always terminal default.
pub(crate) fn style_line_bg(_kind: DiffLineType, _style_context: DiffRenderStyleContext) -> RatatuiStyle {
    RatatuiStyle::default()
}

fn scope_backgrounds_for_level(level: DiffColorLevel) -> DiffScopeBackgroundRgbs {
    match level {
        DiffColorLevel::Ansi16 => DiffScopeBackgroundRgbs::default(),
        DiffColorLevel::TrueColor | DiffColorLevel::Ansi256 => diff_scope_background_rgbs(),
    }
}

// ── Private colour helpers ──────────────────────────────────────────────────

/// Resolve the foreground colour for a diff indicator (gutter/sign).
fn indicator_fg(kind: DiffLineType, theme: DiffTheme) -> Option<RatatuiColor> {
    match (kind, theme) {
        (DiffLineType::Insert, DiffTheme::Dark) => Some(INSERTION_FG_DARK),
        (DiffLineType::Delete, DiffTheme::Dark) => Some(DELETION_FG_DARK),
        (DiffLineType::Insert, DiffTheme::Light) => Some(INSERTION_FG_LIGHT),
        (DiffLineType::Delete, DiffTheme::Light) => Some(DELETION_FG_LIGHT),
        (DiffLineType::Context, _) => None,
    }
}

/// Dim muted gutter foreground so line numbers recede behind content.
/// Gutter is always dimmed (both themes); the `+`/`-` sign stays bright.
fn gutter_fg(theme: DiffTheme) -> RatatuiColor {
    match theme {
        DiffTheme::Dark => RatatuiColor::DarkGray,
        DiffTheme::Light => RatatuiColor::Gray,
    }
}

// ── Public style API ────────────────────────────────────────────────────────

/// Gutter (line number + `│`) style: dimmed muted foreground, no background.
///
/// Dim on both themes so numbers recede while `+`/`-` content stays bright
/// for easy comparison.
pub(crate) fn style_gutter(kind: DiffLineType, style_context: DiffRenderStyleContext) -> RatatuiStyle {
    let _ = kind;
    RatatuiStyle::default()
        .fg(gutter_fg(style_context.theme))
        .add_modifier(Modifier::DIM)
}

/// Sign character (`+`/`-`) style: bright bold foreground, no background.
pub(crate) fn style_sign(kind: DiffLineType, style_context: DiffRenderStyleContext) -> RatatuiStyle {
    let mut style = RatatuiStyle::default().add_modifier(Modifier::BOLD);
    if let Some(color) = indicator_fg(kind, style_context.theme) {
        style = style.fg(color);
    }
    style
}

/// Hunk header (`@@ -old +new @@`): bold cyan foreground only, no background.
pub(crate) fn style_hunk_header(_style_context: DiffRenderStyleContext) -> RatatuiStyle {
    RatatuiStyle::default().fg(RatatuiColor::Cyan).add_modifier(Modifier::BOLD)
}

/// File header for `--- a/path` (red) and `+++ b/path` (green): bold foreground only.
fn style_file_header(kind: DiffLineType, style_context: DiffRenderStyleContext) -> RatatuiStyle {
    let mut style = RatatuiStyle::default().add_modifier(Modifier::BOLD);
    if let Some(color) = indicator_fg(kind, style_context.theme) {
        style = style.fg(color);
    }
    style
}

/// `--- a/path` header: bold red on the deletion tint.
pub(crate) fn style_file_header_old(style_context: DiffRenderStyleContext) -> RatatuiStyle {
    style_file_header(DiffLineType::Delete, style_context)
}

/// `+++ b/path` header: bold green on the addition tint.
pub(crate) fn style_file_header_new(style_context: DiffRenderStyleContext) -> RatatuiStyle {
    style_file_header(DiffLineType::Insert, style_context)
}

/// Content style for plain (non-syntax-highlighted) diff lines.
///
/// Foreground-only: additions and deletions use bright red/green foreground
/// aligned with the gutter marker so body text never diverges from the sign.
pub(crate) fn style_content(kind: DiffLineType, style_context: DiffRenderStyleContext) -> RatatuiStyle {
    let fg = indicator_fg(kind, style_context.theme);
    match kind {
        DiffLineType::Context => RatatuiStyle::default(),
        DiffLineType::Insert | DiffLineType::Delete => fg.map(|c| RatatuiStyle::default().fg(c)).unwrap_or_default(),
    }
}

/// Markdown rendering uses `anstyle`; return the plain diff style there.
pub(crate) fn style_content_ansi(kind: DiffLineType, style_context: DiffRenderStyleContext) -> AnstyleStyle {
    let foreground = indicator_fg(kind, style_context.theme).map(ratatui_color_to_anstyle);
    match kind {
        DiffLineType::Context => AnstyleStyle::new(),
        DiffLineType::Insert | DiffLineType::Delete => AnstyleStyle::new().fg_color(foreground),
    }
}

/// Markdown rendering uses `anstyle`; return the diff marker style there.
///
/// Bright bold foreground only — never dimmed so `+`/`-` stay
/// scannable against the dimmed gutter.
pub(crate) fn style_sign_ansi(kind: DiffLineType, style_context: DiffRenderStyleContext) -> AnstyleStyle {
    let foreground = indicator_fg(kind, style_context.theme).map(ratatui_color_to_anstyle);
    let mut style = AnstyleStyle::new().fg_color(foreground);
    if !matches!(kind, DiffLineType::Context) {
        style = style.effects(anstyle::Effects::BOLD);
    }
    style
}

/// Dimmed gutter style for `anstyle` rendering (line numbers + `│`): no background.
pub(crate) fn style_gutter_ansi(kind: DiffLineType, style_context: DiffRenderStyleContext) -> AnstyleStyle {
    let _ = kind;
    let foreground = Some(ratatui_color_to_anstyle(gutter_fg(style_context.theme)));
    AnstyleStyle::new().fg_color(foreground).effects(anstyle::Effects::DIMMED)
}

/// Hunk header for `anstyle` rendering: bold cyan foreground only, no background.
pub(crate) fn style_hunk_header_ansi(_style_context: DiffRenderStyleContext) -> AnstyleStyle {
    AnstyleStyle::new()
        .fg_color(Some(AnstyleColor::Ansi(anstyle::AnsiColor::Cyan)))
        .effects(anstyle::Effects::BOLD)
}

/// File header for `anstyle` rendering (`---` red, `+++` green): bold foreground only.
fn style_file_header_ansi(kind: DiffLineType, style_context: DiffRenderStyleContext) -> AnstyleStyle {
    let foreground = indicator_fg(kind, style_context.theme).map(ratatui_color_to_anstyle);
    AnstyleStyle::new().fg_color(foreground).effects(anstyle::Effects::BOLD)
}

/// `--- a/path` header for `anstyle` rendering.
pub(crate) fn style_file_header_old_ansi(style_context: DiffRenderStyleContext) -> AnstyleStyle {
    style_file_header_ansi(DiffLineType::Delete, style_context)
}

/// `+++ b/path` header for `anstyle` rendering.
pub(crate) fn style_file_header_new_ansi(style_context: DiffRenderStyleContext) -> AnstyleStyle {
    style_file_header_ansi(DiffLineType::Insert, style_context)
}

fn ratatui_color_to_anstyle(color: RatatuiColor) -> AnstyleColor {
    match color {
        RatatuiColor::Reset => AnstyleColor::Ansi(anstyle::AnsiColor::Black),
        RatatuiColor::Black => AnstyleColor::Ansi(anstyle::AnsiColor::Black),
        RatatuiColor::Red => AnstyleColor::Ansi(anstyle::AnsiColor::Red),
        RatatuiColor::Green => AnstyleColor::Ansi(anstyle::AnsiColor::Green),
        RatatuiColor::Yellow => AnstyleColor::Ansi(anstyle::AnsiColor::Yellow),
        RatatuiColor::Blue => AnstyleColor::Ansi(anstyle::AnsiColor::Blue),
        RatatuiColor::Magenta => AnstyleColor::Ansi(anstyle::AnsiColor::Magenta),
        RatatuiColor::Cyan => AnstyleColor::Ansi(anstyle::AnsiColor::Cyan),
        RatatuiColor::Gray => AnstyleColor::Ansi(anstyle::AnsiColor::White),
        RatatuiColor::DarkGray => AnstyleColor::Ansi(anstyle::AnsiColor::BrightBlack),
        RatatuiColor::LightRed => AnstyleColor::Ansi(anstyle::AnsiColor::BrightRed),
        RatatuiColor::LightGreen => AnstyleColor::Ansi(anstyle::AnsiColor::BrightGreen),
        RatatuiColor::LightYellow => AnstyleColor::Ansi(anstyle::AnsiColor::BrightYellow),
        RatatuiColor::LightBlue => AnstyleColor::Ansi(anstyle::AnsiColor::BrightBlue),
        RatatuiColor::LightMagenta => AnstyleColor::Ansi(anstyle::AnsiColor::BrightMagenta),
        RatatuiColor::LightCyan => AnstyleColor::Ansi(anstyle::AnsiColor::BrightCyan),
        RatatuiColor::White => AnstyleColor::Ansi(anstyle::AnsiColor::BrightWhite),
        RatatuiColor::Indexed(index) => AnstyleColor::Ansi256(anstyle::Ansi256Color(index)),
        RatatuiColor::Rgb(red, green, blue) => AnstyleColor::Rgb(anstyle::RgbColor(red, green, blue)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_style_context(theme: DiffTheme, level: DiffColorLevel) -> DiffRenderStyleContext {
        diff_render_style_context_for(theme, level, scope_backgrounds_for_level(level))
    }

    #[test]
    fn dark_add_bg_is_subtle_green_tint() {
        let bg = diff_add_bg(DiffTheme::Dark, DiffColorLevel::TrueColor);
        assert_eq!(bg, anstyle::Color::Rgb(anstyle::RgbColor(20, 58, 45)));
    }

    #[test]
    fn dark_del_bg_is_subtle_red_tint() {
        let bg = diff_del_bg(DiffTheme::Dark, DiffColorLevel::TrueColor);
        assert_eq!(bg, anstyle::Color::Rgb(anstyle::RgbColor(70, 38, 42)));
    }

    #[test]
    fn light_add_bg_is_subtle_green_tint() {
        let bg = diff_add_bg(DiffTheme::Light, DiffColorLevel::TrueColor);
        assert_eq!(bg, anstyle::Color::Rgb(anstyle::RgbColor(218, 246, 225)));
    }

    #[test]
    fn light_del_bg_is_subtle_red_tint() {
        let bg = diff_del_bg(DiffTheme::Light, DiffColorLevel::TrueColor);
        assert_eq!(bg, anstyle::Color::Rgb(anstyle::RgbColor(255, 224, 224)));
    }

    #[test]
    fn all_levels_use_same_theme_tints() {
        for level in [
            DiffColorLevel::TrueColor,
            DiffColorLevel::Ansi256,
            DiffColorLevel::Ansi16,
        ] {
            assert_eq!(diff_add_bg(DiffTheme::Dark, level), anstyle::Color::Rgb(anstyle::RgbColor(20, 58, 45)));
            assert_eq!(diff_del_bg(DiffTheme::Dark, level), anstyle::Color::Rgb(anstyle::RgbColor(70, 38, 42)));
        }
    }

    #[test]
    fn context_line_bg_is_default() {
        let style =
            style_line_bg(DiffLineType::Context, test_style_context(DiffTheme::Dark, DiffColorLevel::TrueColor));
        assert_eq!(style, RatatuiStyle::default());
    }

    #[test]
    fn dark_gutter_context_is_dimmed_without_bg() {
        let ctx = test_style_context(DiffTheme::Dark, DiffColorLevel::TrueColor);
        let style = style_gutter(DiffLineType::Context, ctx);
        assert!(style.add_modifier.contains(Modifier::DIM));
        assert_eq!(style.bg, None);
    }

    #[test]
    fn insert_gutter_is_dimmed_muted_without_bg() {
        let ctx = test_style_context(DiffTheme::Dark, DiffColorLevel::TrueColor);
        let style = style_gutter(DiffLineType::Insert, ctx);
        assert_eq!(style.fg, Some(RatatuiColor::DarkGray));
        assert!(style.add_modifier.contains(Modifier::DIM));
        assert_eq!(style.bg, None);
    }

    #[test]
    fn delete_gutter_is_dimmed_muted_without_bg() {
        let ctx = test_style_context(DiffTheme::Dark, DiffColorLevel::TrueColor);
        let style = style_gutter(DiffLineType::Delete, ctx);
        assert_eq!(style.fg, Some(RatatuiColor::DarkGray));
        assert!(style.add_modifier.contains(Modifier::DIM));
        assert_eq!(style.bg, None);
    }

    #[test]
    fn dark_ansi16_content_uses_foreground_only() {
        let style = style_content(DiffLineType::Insert, test_style_context(DiffTheme::Dark, DiffColorLevel::Ansi16));
        assert_eq!(style.fg, Some(RatatuiColor::LightGreen));
        assert_eq!(style.bg, None);
    }

    #[test]
    fn truecolor_content_uses_foreground_only_without_tint() {
        let ctx = test_style_context(DiffTheme::Dark, DiffColorLevel::TrueColor);
        let add = style_content(DiffLineType::Insert, ctx);
        let del = style_content(DiffLineType::Delete, ctx);
        assert_eq!(add.fg, Some(RatatuiColor::LightGreen));
        assert_eq!(del.fg, Some(RatatuiColor::Rgb(255, 90, 90)));
        assert_eq!(add.bg, None);
        assert_eq!(del.bg, None);
        let sign = style_sign(DiffLineType::Insert, ctx);
        assert_eq!(sign.fg, Some(RatatuiColor::LightGreen));
        assert_eq!(sign.fg, add.fg);
        assert_eq!(sign.bg, None);
    }

    #[test]
    fn sign_style_dark_uses_light_green_and_custom_red_with_bold_without_bg() {
        let ctx = test_style_context(DiffTheme::Dark, DiffColorLevel::TrueColor);
        let add_sign = style_sign(DiffLineType::Insert, ctx);
        let del_sign = style_sign(DiffLineType::Delete, ctx);
        assert_eq!(add_sign.fg, Some(RatatuiColor::LightGreen));
        assert_eq!(del_sign.fg, Some(RatatuiColor::Rgb(255, 90, 90)));
        assert!(add_sign.add_modifier.contains(Modifier::BOLD));
        assert!(del_sign.add_modifier.contains(Modifier::BOLD));
        assert_eq!(add_sign.bg, None);
        assert_eq!(del_sign.bg, None);
    }

    #[test]
    fn sign_style_light_stays_bright_bold_without_dim() {
        let ctx = test_style_context(DiffTheme::Light, DiffColorLevel::TrueColor);
        let add_sign = style_sign(DiffLineType::Insert, ctx);
        let del_sign = style_sign(DiffLineType::Delete, ctx);
        assert_eq!(add_sign.fg, Some(RatatuiColor::LightGreen));
        assert_eq!(del_sign.fg, Some(RatatuiColor::LightRed));
        assert!(add_sign.add_modifier.contains(Modifier::BOLD));
        assert!(!add_sign.add_modifier.contains(Modifier::DIM));
        assert!(!del_sign.add_modifier.contains(Modifier::DIM));
    }

    #[test]
    fn hunk_header_uses_cyan_bold_without_bg() {
        let ctx = test_style_context(DiffTheme::Dark, DiffColorLevel::TrueColor);
        let style = style_hunk_header(ctx);
        assert_eq!(style.fg, Some(RatatuiColor::Cyan));
        assert!(style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(style.bg, None);
    }

    #[test]
    fn file_headers_use_red_green_bold_without_bg() {
        let ctx = test_style_context(DiffTheme::Dark, DiffColorLevel::TrueColor);
        let old = style_file_header_old(ctx);
        let new = style_file_header_new(ctx);
        assert!(old.add_modifier.contains(Modifier::BOLD));
        assert!(new.add_modifier.contains(Modifier::BOLD));
        assert_eq!(old.bg, None);
        assert_eq!(new.bg, None);
        assert_ne!(old.fg, new.fg);
    }

    #[test]
    fn line_backgrounds_stay_default_even_with_scope_colors() {
        let style_context = diff_render_style_context_for(
            DiffTheme::Dark,
            DiffColorLevel::TrueColor,
            DiffScopeBackgroundRgbs {
                inserted: Some((1, 2, 3)),
                deleted: Some((4, 5, 6)),
            },
        );

        assert_eq!(style_line_bg(DiffLineType::Insert, style_context), RatatuiStyle::default());
        assert_eq!(style_line_bg(DiffLineType::Delete, style_context), RatatuiStyle::default());
    }

    #[test]
    fn ansi256_keeps_foreground_only_line_backgrounds() {
        let style_context = diff_render_style_context_for(
            DiffTheme::Dark,
            DiffColorLevel::Ansi256,
            DiffScopeBackgroundRgbs { inserted: Some((0, 95, 0)), deleted: None },
        );
        assert_eq!(style_line_bg(DiffLineType::Insert, style_context), RatatuiStyle::default());
        assert_eq!(style_line_bg(DiffLineType::Delete, style_context), RatatuiStyle::default());
    }

    #[test]
    fn ansi16_disables_line_backgrounds_even_with_scope_colors() {
        let style_context = diff_render_style_context_for(
            DiffTheme::Dark,
            DiffColorLevel::Ansi16,
            DiffScopeBackgroundRgbs {
                inserted: Some((8, 9, 10)),
                deleted: Some((11, 12, 13)),
            },
        );
        assert_eq!(style_line_bg(DiffLineType::Insert, style_context), RatatuiStyle::default());
        assert_eq!(style_line_bg(DiffLineType::Delete, style_context), RatatuiStyle::default());
    }

    #[test]
    fn ansi16_content_has_no_background() {
        let style_context =
            diff_render_style_context_for(DiffTheme::Dark, DiffColorLevel::Ansi16, DiffScopeBackgroundRgbs::default());
        let add = style_content(DiffLineType::Insert, style_context);
        let del = style_content(DiffLineType::Delete, style_context);
        assert_eq!(add.fg, Some(INSERTION_FG_DARK));
        assert_eq!(add.bg, None);
        assert_eq!(del.fg, Some(DELETION_FG_DARK));
        assert_eq!(del.bg, None);
    }

    #[test]
    fn partial_scope_override_keeps_foreground_only_styles() {
        let style_context = diff_render_style_context_for(
            DiffTheme::Dark,
            DiffColorLevel::TrueColor,
            DiffScopeBackgroundRgbs { inserted: Some((12, 34, 56)), deleted: None },
        );
        assert_eq!(style_line_bg(DiffLineType::Insert, style_context), RatatuiStyle::default());
        assert_eq!(style_content(DiffLineType::Insert, style_context).bg, None);
        assert_eq!(style_content(DiffLineType::Delete, style_context).bg, None);
    }
}
