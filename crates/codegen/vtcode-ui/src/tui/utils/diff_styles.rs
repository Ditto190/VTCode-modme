//! Unified diff styles for TUI rendering
//!
//! Re-exports diff theme from vtcode-commons and provides
//! ratatui-specific style helpers for diff rendering.

// Re-export diff theme from vtcode-commons
pub use vtcode_commons::diff_theme::{
    DiffColorLevel, DiffTheme, diff_add_bg, diff_add_fg, diff_add_word_bg, diff_del_bg, diff_del_fg, diff_del_word_bg,
    diff_gutter_bg_add_light, diff_gutter_bg_del_light, diff_gutter_fg, diff_gutter_fg_light, diff_hunk_bg,
};
pub use vtcode_commons::styling::DiffColorPalette;

use crate::tui::ui::syntax_highlight::{DiffScopeBackgroundRgbs, diff_scope_background_rgbs};
use ratatui::style::{Color as RatatuiColor, Modifier, Style as RatatuiStyle};
use vtcode_commons::color256_theme::rgb_to_ansi256_for_theme;

use anstyle::{Color as AnstyleColor, Style as AnstyleStyle};

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
struct ResolvedDiffBackgrounds {
    add: Option<RatatuiColor>,
    del: Option<RatatuiColor>,
}

/// Snapshot of diff styling inputs that can be reused while rendering.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiffRenderStyleContext {
    theme: DiffTheme,
    level: DiffColorLevel,
    backgrounds: ResolvedDiffBackgrounds,
}

impl DiffRenderStyleContext {
    pub fn theme(self) -> DiffTheme {
        self.theme
    }

    pub fn level(self) -> DiffColorLevel {
        self.level
    }

    /// Stronger addition background for changed intraline spans.
    pub fn add_word_bg(self) -> Option<AnstyleColor> {
        (self.level != DiffColorLevel::Ansi16).then(|| diff_add_word_bg(self.theme, self.level))
    }

    /// Stronger deletion background for changed intraline spans.
    pub fn del_word_bg(self) -> Option<AnstyleColor> {
        (self.level != DiffColorLevel::Ansi16).then(|| diff_del_word_bg(self.theme, self.level))
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

fn content_background(kind: DiffLineType, style_context: DiffRenderStyleContext) -> Option<RatatuiColor> {
    match kind {
        DiffLineType::Insert => style_context.backgrounds.add,
        DiffLineType::Delete => style_context.backgrounds.del,
        DiffLineType::Context => None,
    }
}

fn word_background(kind: DiffLineType, style_context: DiffRenderStyleContext) -> Option<RatatuiColor> {
    let color = match kind {
        DiffLineType::Insert => style_context.add_word_bg(),
        DiffLineType::Delete => style_context.del_word_bg(),
        DiffLineType::Context => None,
    }?;
    Some(ratatui_color_from_anstyle(color))
}

/// Full-width line background style. Context rows and ANSI16 terminals use
/// the terminal default background.
pub(crate) fn style_line_bg(kind: DiffLineType, style_context: DiffRenderStyleContext) -> RatatuiStyle {
    content_background(kind, style_context).map_or_else(RatatuiStyle::default, |bg| RatatuiStyle::default().bg(bg))
}

/// Stronger background for a changed intraline span.
pub(crate) fn style_word_bg(kind: DiffLineType, style_context: DiffRenderStyleContext) -> RatatuiStyle {
    word_background(kind, style_context).map_or_else(RatatuiStyle::default, |bg| RatatuiStyle::default().bg(bg))
}

fn scope_backgrounds_for_level(level: DiffColorLevel) -> DiffScopeBackgroundRgbs {
    match level {
        DiffColorLevel::Ansi16 => DiffScopeBackgroundRgbs::default(),
        DiffColorLevel::TrueColor | DiffColorLevel::Ansi256 => diff_scope_background_rgbs(),
    }
}

// ── Private colour helpers ──────────────────────────────────────────────────

/// Resolve the foreground colour for a diff indicator (gutter/sign).
fn indicator_fg(kind: DiffLineType, style_context: DiffRenderStyleContext) -> Option<RatatuiColor> {
    let color = match kind {
        DiffLineType::Insert => diff_add_fg(style_context.theme, style_context.level),
        DiffLineType::Delete => diff_del_fg(style_context.theme, style_context.level),
        DiffLineType::Context => return None,
    };
    Some(ratatui_color_from_anstyle(color))
}

/// Theme-aware gutter foreground. It is intentionally explicit rather than
/// DIMMED so line numbers remain readable on both diff background levels.
fn gutter_fg(style_context: DiffRenderStyleContext) -> RatatuiColor {
    ratatui_color_from_anstyle(diff_gutter_fg(style_context.theme, style_context.level))
}

// ── Public style API ────────────────────────────────────────────────────────

/// Gutter (line number + `│`) style: muted, contrast-safe foreground on the
/// row tint. The explicit theme colors recede without applying DIM, whose
/// terminal-dependent attenuation can make tinted gutters unreadable.
pub(crate) fn style_gutter(kind: DiffLineType, style_context: DiffRenderStyleContext) -> RatatuiStyle {
    let mut style = RatatuiStyle::default().fg(gutter_fg(style_context));
    if let Some(bg) = content_background(kind, style_context) {
        style = style.bg(bg);
    }
    style
}

/// Sign character (`+`/`-`) style: bright bold foreground on the row tint.
pub(crate) fn style_sign(kind: DiffLineType, style_context: DiffRenderStyleContext) -> RatatuiStyle {
    let mut style = RatatuiStyle::default().add_modifier(Modifier::BOLD);
    if let Some(color) = indicator_fg(kind, style_context) {
        style = style.fg(color);
    }
    if let Some(bg) = content_background(kind, style_context) {
        style = style.bg(bg);
    }
    style
}

/// Hunk header (`@@ -old +new @@`): bold cyan metadata with no row tint.
pub(crate) fn style_hunk_header(style_context: DiffRenderStyleContext) -> RatatuiStyle {
    let _ = style_context;
    RatatuiStyle::default().fg(RatatuiColor::Cyan).add_modifier(Modifier::BOLD)
}

/// File header for `--- a/path` (red) and `+++ b/path` (green): bold
/// foreground-only metadata. File headers are section labels, not diff rows.
fn style_file_header(kind: DiffLineType, style_context: DiffRenderStyleContext) -> RatatuiStyle {
    let mut style = RatatuiStyle::default().add_modifier(Modifier::BOLD);
    if let Some(color) = indicator_fg(kind, style_context) {
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
/// Color-capable terminals use the row tint with the terminal's normal
/// foreground. ANSI16 retains the bright foreground fallback because it
/// cannot represent the tint.
pub(crate) fn style_content(kind: DiffLineType, style_context: DiffRenderStyleContext) -> RatatuiStyle {
    let bg = content_background(kind, style_context);
    let fg = indicator_fg(kind, style_context);
    match (kind, style_context.level, bg) {
        (DiffLineType::Context, _, _) => RatatuiStyle::default(),
        (_, DiffColorLevel::Ansi16, _) => fg.map(|c| RatatuiStyle::default().fg(c)).unwrap_or_default(),
        (_, _, Some(bg)) => RatatuiStyle::default().bg(bg),
        (_, _, None) => RatatuiStyle::default(),
    }
}

/// Markdown rendering uses `anstyle`; return the plain diff style there.
pub(crate) fn style_content_ansi(kind: DiffLineType, style_context: DiffRenderStyleContext) -> AnstyleStyle {
    let background = content_background(kind, style_context).map(ratatui_color_to_anstyle);
    let foreground = indicator_fg(kind, style_context).map(ratatui_color_to_anstyle);
    match (kind, style_context.level, background) {
        (DiffLineType::Context, _, _) => AnstyleStyle::new(),
        (_, DiffColorLevel::Ansi16, _) => AnstyleStyle::new().fg_color(foreground),
        (_, _, background) => AnstyleStyle::new().bg_color(background),
    }
}

/// Markdown rendering uses `anstyle`; return the diff marker style there.
///
/// Bright bold foreground on the row tint — never dimmed so `+`/`-` stay
/// scannable against the muted gutter.
pub(crate) fn style_sign_ansi(kind: DiffLineType, style_context: DiffRenderStyleContext) -> AnstyleStyle {
    let background = content_background(kind, style_context).map(ratatui_color_to_anstyle);
    let foreground = indicator_fg(kind, style_context).map(ratatui_color_to_anstyle);
    let mut style = AnstyleStyle::new().fg_color(foreground).bg_color(background);
    if !matches!(kind, DiffLineType::Context) {
        style = style.effects(anstyle::Effects::BOLD);
    }
    style
}

/// Contrast-safe gutter style for `anstyle` rendering (line numbers + `│`) on
/// the row tint. Avoid DIM because its result depends on the terminal palette.
pub(crate) fn style_gutter_ansi(kind: DiffLineType, style_context: DiffRenderStyleContext) -> AnstyleStyle {
    let background = content_background(kind, style_context).map(ratatui_color_to_anstyle);
    let foreground = Some(ratatui_color_to_anstyle(gutter_fg(style_context)));
    AnstyleStyle::new().fg_color(foreground).bg_color(background)
}

/// Hunk header for `anstyle` rendering: bold cyan metadata without a
/// background.
pub(crate) fn style_hunk_header_ansi(style_context: DiffRenderStyleContext) -> AnstyleStyle {
    let _ = style_context;
    AnstyleStyle::new()
        .fg_color(Some(AnstyleColor::Ansi(anstyle::AnsiColor::Cyan)))
        .effects(anstyle::Effects::BOLD)
}

/// File header for `anstyle` rendering (`---` red, `+++` green): bold
/// foreground-only metadata.
fn style_file_header_ansi(kind: DiffLineType, style_context: DiffRenderStyleContext) -> AnstyleStyle {
    let foreground = indicator_fg(kind, style_context).map(ratatui_color_to_anstyle);
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
        diff_render_style_context_for(theme, level, DiffScopeBackgroundRgbs::default())
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
    fn backgrounds_follow_terminal_capability() {
        assert_eq!(
            diff_add_bg(DiffTheme::Dark, DiffColorLevel::TrueColor),
            anstyle::Color::Rgb(anstyle::RgbColor(20, 58, 45))
        );
        assert_eq!(
            diff_del_bg(DiffTheme::Dark, DiffColorLevel::TrueColor),
            anstyle::Color::Rgb(anstyle::RgbColor(70, 38, 42))
        );
        assert!(matches!(diff_add_bg(DiffTheme::Dark, DiffColorLevel::Ansi256), anstyle::Color::Ansi256(_)));
        assert!(matches!(diff_del_word_bg(DiffTheme::Light, DiffColorLevel::Ansi256), anstyle::Color::Ansi256(_)));
    }

    #[test]
    fn context_line_bg_is_default() {
        let style =
            style_line_bg(DiffLineType::Context, test_style_context(DiffTheme::Dark, DiffColorLevel::TrueColor));
        assert_eq!(style, RatatuiStyle::default());
    }

    #[test]
    fn dark_gutter_context_is_contrast_safe_without_bg() {
        let ctx = test_style_context(DiffTheme::Dark, DiffColorLevel::TrueColor);
        let style = style_gutter(DiffLineType::Context, ctx);
        assert_eq!(style.fg, Some(RatatuiColor::Rgb(165, 175, 170)));
        assert!(!style.add_modifier.contains(Modifier::DIM));
        assert_eq!(style.bg, None);
    }

    #[test]
    fn insert_gutter_is_contrast_safe_on_add_bg() {
        let ctx = test_style_context(DiffTheme::Dark, DiffColorLevel::TrueColor);
        let style = style_gutter(DiffLineType::Insert, ctx);
        assert_eq!(style.fg, Some(RatatuiColor::Rgb(165, 175, 170)));
        assert!(!style.add_modifier.contains(Modifier::DIM));
        assert_eq!(style.bg, Some(RatatuiColor::Rgb(20, 58, 45)));
    }

    #[test]
    fn delete_gutter_is_contrast_safe_on_delete_bg() {
        let ctx = test_style_context(DiffTheme::Dark, DiffColorLevel::TrueColor);
        let style = style_gutter(DiffLineType::Delete, ctx);
        assert_eq!(style.fg, Some(RatatuiColor::Rgb(165, 175, 170)));
        assert!(!style.add_modifier.contains(Modifier::DIM));
        assert_eq!(style.bg, Some(RatatuiColor::Rgb(70, 38, 42)));
    }

    #[test]
    fn dark_ansi16_content_uses_foreground_only() {
        let style = style_content(DiffLineType::Insert, test_style_context(DiffTheme::Dark, DiffColorLevel::Ansi16));
        assert_eq!(style.fg, Some(RatatuiColor::LightGreen));
        assert_eq!(style.bg, None);
    }

    #[test]
    fn truecolor_content_uses_row_tint_and_keeps_default_foreground() {
        let ctx = test_style_context(DiffTheme::Dark, DiffColorLevel::TrueColor);
        let add = style_content(DiffLineType::Insert, ctx);
        let del = style_content(DiffLineType::Delete, ctx);
        assert_eq!(add.fg, None);
        assert_eq!(del.fg, None);
        assert_eq!(add.bg, Some(RatatuiColor::Rgb(20, 58, 45)));
        assert_eq!(del.bg, Some(RatatuiColor::Rgb(70, 38, 42)));
        let sign = style_sign(DiffLineType::Insert, ctx);
        assert_eq!(sign.fg, Some(RatatuiColor::Rgb(85, 255, 85)));
        assert_eq!(sign.bg, add.bg);
    }

    #[test]
    fn sign_style_dark_uses_light_green_and_custom_red_with_bold_on_row_bg() {
        let ctx = test_style_context(DiffTheme::Dark, DiffColorLevel::TrueColor);
        let add_sign = style_sign(DiffLineType::Insert, ctx);
        let del_sign = style_sign(DiffLineType::Delete, ctx);
        assert_eq!(add_sign.fg, Some(RatatuiColor::Rgb(85, 255, 85)));
        assert_eq!(del_sign.fg, Some(RatatuiColor::Rgb(255, 180, 180)));
        assert!(add_sign.add_modifier.contains(Modifier::BOLD));
        assert!(del_sign.add_modifier.contains(Modifier::BOLD));
        assert_eq!(add_sign.bg, Some(RatatuiColor::Rgb(20, 58, 45)));
        assert_eq!(del_sign.bg, Some(RatatuiColor::Rgb(70, 38, 42)));
    }

    #[test]
    fn sign_style_light_stays_bright_bold_on_row_bg() {
        let ctx = test_style_context(DiffTheme::Light, DiffColorLevel::TrueColor);
        let add_sign = style_sign(DiffLineType::Insert, ctx);
        let del_sign = style_sign(DiffLineType::Delete, ctx);
        assert_eq!(add_sign.fg, Some(RatatuiColor::Rgb(0, 92, 43)));
        assert_eq!(del_sign.fg, Some(RatatuiColor::Rgb(140, 20, 25)));
        assert!(add_sign.add_modifier.contains(Modifier::BOLD));
        assert!(!add_sign.add_modifier.contains(Modifier::DIM));
        assert!(!del_sign.add_modifier.contains(Modifier::DIM));
        assert_eq!(add_sign.bg, Some(RatatuiColor::Rgb(218, 246, 225)));
        assert_eq!(del_sign.bg, Some(RatatuiColor::Rgb(255, 224, 224)));
    }

    #[test]
    fn hunk_header_uses_cyan_bold_without_background() {
        let ctx = test_style_context(DiffTheme::Dark, DiffColorLevel::TrueColor);
        let style = style_hunk_header(ctx);
        assert_eq!(style.fg, Some(RatatuiColor::Cyan));
        assert!(style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(style.bg, None);
    }

    #[test]
    fn file_headers_use_red_green_bold_without_background() {
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
    fn line_backgrounds_use_scope_colors_when_available() {
        let style_context = diff_render_style_context_for(
            DiffTheme::Dark,
            DiffColorLevel::TrueColor,
            DiffScopeBackgroundRgbs {
                inserted: Some((1, 2, 3)),
                deleted: Some((4, 5, 6)),
            },
        );

        assert_eq!(style_line_bg(DiffLineType::Insert, style_context).bg, Some(RatatuiColor::Rgb(1, 2, 3)));
        assert_eq!(style_line_bg(DiffLineType::Delete, style_context).bg, Some(RatatuiColor::Rgb(4, 5, 6)));
    }

    #[test]
    fn ansi256_maps_row_and_word_backgrounds() {
        let style_context = diff_render_style_context_for(
            DiffTheme::Dark,
            DiffColorLevel::Ansi256,
            DiffScopeBackgroundRgbs { inserted: Some((0, 95, 0)), deleted: None },
        );
        assert!(style_line_bg(DiffLineType::Insert, style_context).bg.is_some());
        assert!(style_line_bg(DiffLineType::Delete, style_context).bg.is_some());
        assert!(style_word_bg(DiffLineType::Insert, style_context).bg.is_some());
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
        assert_eq!(add.fg, Some(RatatuiColor::LightGreen));
        assert_eq!(add.bg, None);
        assert_eq!(del.fg, Some(RatatuiColor::LightRed));
        assert_eq!(del.bg, None);
    }

    #[test]
    fn partial_scope_override_keeps_palette_for_missing_side() {
        let style_context = diff_render_style_context_for(
            DiffTheme::Dark,
            DiffColorLevel::TrueColor,
            DiffScopeBackgroundRgbs { inserted: Some((12, 34, 56)), deleted: None },
        );
        assert_eq!(style_line_bg(DiffLineType::Insert, style_context).bg, Some(RatatuiColor::Rgb(12, 34, 56)));
        assert_eq!(style_content(DiffLineType::Insert, style_context).bg, Some(RatatuiColor::Rgb(12, 34, 56)));
        assert_eq!(style_content(DiffLineType::Delete, style_context).bg, Some(RatatuiColor::Rgb(70, 38, 42)));
    }

    #[test]
    fn word_background_is_stronger_and_ansi16_stays_foreground_only() {
        let ctx = test_style_context(DiffTheme::Dark, DiffColorLevel::TrueColor);
        assert_eq!(style_word_bg(DiffLineType::Insert, ctx).bg, Some(RatatuiColor::Rgb(36, 100, 70)));
        assert_eq!(style_word_bg(DiffLineType::Delete, ctx).bg, Some(RatatuiColor::Rgb(140, 52, 58)));

        let ansi16 = test_style_context(DiffTheme::Dark, DiffColorLevel::Ansi16);
        assert_eq!(style_line_bg(DiffLineType::Insert, ansi16).bg, None);
        assert_eq!(style_word_bg(DiffLineType::Insert, ansi16).bg, None);
        assert_eq!(style_content_ansi(DiffLineType::Insert, ansi16).get_bg_color(), None);
        assert_eq!(style_sign_ansi(DiffLineType::Insert, ansi16).get_bg_color(), None);
    }

    fn relative_luminance(color: (u8, u8, u8)) -> f32 {
        let linear = |component: u8| {
            let value = f32::from(component) / 255.0;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        let [red, green, blue] = [linear(color.0), linear(color.1), linear(color.2)];
        0.2126 * red + 0.7152 * green + 0.0722 * blue
    }

    fn contrast_ratio(foreground: (u8, u8, u8), background: (u8, u8, u8)) -> f32 {
        let foreground = relative_luminance(foreground);
        let background = relative_luminance(background);
        (foreground.max(background) + 0.05) / (foreground.min(background) + 0.05)
    }

    fn test_color_rgb(color: RatatuiColor) -> (u8, u8, u8) {
        match color {
            RatatuiColor::Rgb(red, green, blue) => (red, green, blue),
            RatatuiColor::LightGreen => (85, 255, 85),
            other => panic!("test only covers RGB and bright-green marker colors, got {other:?}"),
        }
    }

    #[test]
    fn marker_foregrounds_meet_wcag_against_both_diff_background_levels() {
        for theme in [DiffTheme::Dark, DiffTheme::Light] {
            let context = test_style_context(theme, DiffColorLevel::TrueColor);
            for kind in [DiffLineType::Insert, DiffLineType::Delete] {
                let foreground = test_color_rgb(style_sign(kind, context).fg.expect("marker foreground"));
                let row = test_color_rgb(style_line_bg(kind, context).bg.expect("row background"));
                let word = test_color_rgb(style_word_bg(kind, context).bg.expect("word background"));
                let gutter = test_color_rgb(style_gutter(kind, context).fg.expect("gutter foreground"));
                assert!(
                    contrast_ratio(foreground, row) >= 4.5,
                    "{theme:?} {kind:?} marker fails row contrast: {foreground:?} on {row:?}"
                );
                assert!(
                    contrast_ratio(foreground, word) >= 4.5,
                    "{theme:?} {kind:?} marker fails word contrast: {foreground:?} on {word:?}"
                );
                assert!(
                    contrast_ratio(gutter, row) >= 4.5,
                    "{theme:?} {kind:?} gutter fails row contrast: {gutter:?} on {row:?}"
                );
            }
        }
    }
}
