//! Unified diff formatting with ANSI colors.
//!
//! Provides `format_colored_diff` as the single canonical implementation
//! for rendering diff hunks with terminal colors. Previously duplicated
//! in `vtcode-core` and `vtcode-ui`.

use anstyle::{AnsiColor, Color, Reset, RgbColor, Style};
use std::fmt::Write;
use vtcode_diff::{DiffDocument, format_unified_hunks};

// Keep the published UI facade's legacy options shape while exposing the
// renderer-neutral types from the extracted implementation. New consumers
// that need algorithm or timeout controls should depend on `vtcode-diff`.
pub use vtcode_commons::diff::{DiffOptions, compute_diff};
pub use vtcode_diff::{Chunk, DiffBundle, DiffHunk, DiffLine, DiffLineKind, compute_diff_chunks};

/// Named diff color themes with WCAG AA-checked accents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiffTheme {
    /// Light theme modeled on GitHub's diff colors.
    #[default]
    Github,
    /// Dark theme modeled on Ayu.
    Ayu,
    /// Dark theme modeled on Gruvbox.
    Gruvbox,
    /// Dark theme modeled on Nord.
    Nord,
    /// Light theme modeled on Solarized.
    Solarized,
    /// Dark theme modeled on Dracula.
    Dracula,
}

impl DiffTheme {
    /// Every built-in diff theme, in selection order.
    pub const ALL: [DiffTheme; 6] = [
        DiffTheme::Github,
        DiffTheme::Ayu,
        DiffTheme::Gruvbox,
        DiffTheme::Nord,
        DiffTheme::Solarized,
        DiffTheme::Dracula,
    ];

    /// Stable selection name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Github => "github",
            Self::Ayu => "ayu",
            Self::Gruvbox => "gruvbox",
            Self::Nord => "nord",
            Self::Solarized => "solarized",
            Self::Dracula => "dracula",
        }
    }

    /// Resolves a theme by its stable selection name.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|theme| theme.name() == name)
    }

    fn palette(self) -> DiffPalette {
        match self {
            Self::Github => DiffPalette {
                background: (0xff, 0xff, 0xff),
                header: (0x57, 0x60, 0x6a),
                addition: (0x22, 0x86, 0x3a),
                deletion: (0xcb, 0x24, 0x31),
            },
            Self::Ayu => DiffPalette {
                background: (0x0b, 0x0e, 0x14),
                header: (0x8b, 0x94, 0x9e),
                addition: (0xaa, 0xd9, 0x4c),
                deletion: (0xf0, 0x71, 0x78),
            },
            Self::Gruvbox => DiffPalette {
                background: (0x28, 0x28, 0x28),
                header: (0x83, 0xa5, 0x98),
                addition: (0xb8, 0xbb, 0x26),
                deletion: (0xfc, 0x6b, 0x57),
            },
            Self::Nord => DiffPalette {
                background: (0x2e, 0x34, 0x40),
                header: (0x88, 0xc0, 0xd0),
                addition: (0xa3, 0xbe, 0x8c),
                deletion: (0xeb, 0x8f, 0x97),
            },
            Self::Solarized => DiffPalette {
                background: (0xfd, 0xf6, 0xe3),
                header: (0x58, 0x6e, 0x75),
                addition: (0x5b, 0x73, 0x00),
                deletion: (0xb5, 0x37, 0x2f),
            },
            Self::Dracula => DiffPalette {
                background: (0x28, 0x2a, 0x36),
                header: (0x8b, 0x96, 0xc9),
                addition: (0x50, 0xfa, 0x7b),
                deletion: (0xff, 0x55, 0x55),
            },
        }
    }
}

/// Foreground accents and background for one diff theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiffPalette {
    /// Terminal background the accents are contrast-checked against.
    pub background: (u8, u8, u8),
    /// Hunk header and file label color.
    pub header: (u8, u8, u8),
    /// Added-line color.
    pub addition: (u8, u8, u8),
    /// Deleted-line color.
    pub deletion: (u8, u8, u8),
}

impl DiffPalette {
    fn header_style(self) -> Style {
        rgb_style(self.header)
    }

    fn addition_style(self) -> Style {
        rgb_style(self.addition)
    }

    fn deletion_style(self) -> Style {
        rgb_style(self.deletion)
    }
}

fn rgb_style((red, green, blue): (u8, u8, u8)) -> Style {
    Style::new().fg_color(Some(Color::Rgb(RgbColor(red, green, blue))))
}

/// Format a unified diff without ANSI color codes.
pub fn format_unified_diff(old: &str, new: &str, options: DiffOptions<'_>) -> String {
    let mut options = shared_options(&options);
    options.missing_newline_hint = false;
    let document = DiffDocument::between(old, new, options.clone());
    format_unified_hunks(&document.hunks, &options)
}

/// Compute a structured diff bundle using the default theme-aware formatter.
pub fn compute_diff_with_theme(old: &str, new: &str, options: DiffOptions<'_>) -> DiffBundle {
    let shared = shared_options(&options);
    let document = DiffDocument::between(old, new, shared);
    let formatted = format_colored_diff(&document.hunks, &options);
    DiffBundle {
        hunks: document.hunks,
        formatted,
        is_empty: old == new,
    }
}

fn shared_options<'a>(options: &DiffOptions<'a>) -> vtcode_diff::DiffOptions<'a> {
    vtcode_diff::DiffOptions {
        context_lines: options.context_lines,
        old_label: options.old_label,
        new_label: options.new_label,
        missing_newline_hint: options.missing_newline_hint,
        ..vtcode_diff::DiffOptions::default()
    }
}

/// Format diff hunks with standard ANSI colors for terminal display.
///
/// This is the single canonical implementation. Both `vtcode-core` and
/// `vtcode-ui` delegate to this function.
pub fn format_colored_diff(hunks: &[DiffHunk], options: &DiffOptions<'_>) -> String {
    if hunks.is_empty() {
        return String::new();
    }

    // Keep the canonical portable ANSI-16 colors for the legacy entry point;
    // RGB palettes are opt-in through `format_colored_diff_with_theme`.
    let cyan_style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Cyan)));
    let addition_style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Green)));
    let deletion_style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Red)));
    let context_style = Style::new();
    format_colored_diff_impl(hunks, options, cyan_style, addition_style, deletion_style, context_style)
}

/// Format diff hunks with the colors of a selected diff theme.
///
/// Keeps the canonical `format_colored_diff` behavior for the default theme
/// while allowing selectable themes (github, ayu, gruvbox, nord, solarized,
/// dracula) for diff previews.
pub fn format_colored_diff_with_theme(hunks: &[DiffHunk], options: &DiffOptions<'_>, theme: DiffTheme) -> String {
    if hunks.is_empty() {
        return String::new();
    }

    let palette = theme.palette();
    format_colored_diff_impl(
        hunks,
        options,
        palette.header_style(),
        palette.addition_style(),
        palette.deletion_style(),
        Style::new(),
    )
}

fn format_colored_diff_impl(
    hunks: &[DiffHunk],
    options: &DiffOptions<'_>,
    header_style: Style,
    addition_style: Style,
    deletion_style: Style,
    context_style: Style,
) -> String {
    let mut output = String::new();

    if let (Some(old_label), Some(new_label)) = (options.old_label, options.new_label) {
        let _ = write!(output, "{}--- {old_label}\n{}", header_style.render(), Reset.render());

        let _ = write!(output, "{}+++ {new_label}\n{}", header_style.render(), Reset.render());
    }

    for hunk in hunks {
        let _ = write!(
            output,
            "{}@@ -{},{} +{},{} @@\n{}",
            header_style.render(),
            hunk.old_start,
            hunk.old_lines,
            hunk.new_start,
            hunk.new_lines,
            Reset.render()
        );

        for line in &hunk.lines {
            let (style, prefix) = match line.kind {
                DiffLineKind::Addition => (&addition_style, '+'),
                DiffLineKind::Deletion => (&deletion_style, '-'),
                DiffLineKind::Context => (&context_style, ' '),
            };

            let mut display = String::with_capacity(line.text.len() + 2);
            display.push(prefix);
            display.push_str(&line.text);

            // Apply Reset before the line terminator to prevent color
            // bleeding, and normalize CR/CRLF for terminal output.
            let display_content = display
                .strip_suffix("\r\n")
                .or_else(|| display.strip_suffix('\n'))
                .or_else(|| display.strip_suffix('\r'))
                .unwrap_or(&display);

            let _ = write!(output, "{}{} {}", style.render(), display_content, Reset.render());
            output.push('\n');

            let has_line_terminator = line.text.ends_with('\n') || line.text.ends_with('\r');
            if options.missing_newline_hint && !has_line_terminator {
                let eof_hint = r"\ No newline at end of file";
                let _ = write!(output, "{}{} {}", context_style.render(), eof_hint, Reset.render());
                output.push('\n');
            }
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_structured_diff() {
        let before = "a\nb\nc\n";
        let after = "a\nc\nd\n";
        let bundle = compute_diff(
            before,
            after,
            DiffOptions {
                context_lines: 2,
                old_label: Some("old"),
                new_label: Some("new"),
                ..Default::default()
            },
            format_colored_diff,
        );

        assert!(!bundle.is_empty);
        assert_eq!(bundle.hunks.len(), 1);
        let hunk = &bundle.hunks[0];
        assert_eq!(hunk.old_start, 1);
        assert_eq!(hunk.new_start, 1);
        assert!(bundle.formatted.contains("@@"));
        assert!(hunk.lines.iter().any(|line| matches!(line.kind, DiffLineKind::Deletion)));
        assert!(hunk.lines.iter().any(|line| matches!(line.kind, DiffLineKind::Addition)));
    }

    #[test]
    fn empty_hunks_returns_empty_string() {
        let result = format_colored_diff(&[], &DiffOptions::default());
        assert!(result.is_empty());
    }

    #[test]
    fn format_unified_diff_has_no_ansi() {
        let before = "hello\n";
        let after = "world\n";
        let result = format_unified_diff(before, after, DiffOptions::default());
        // The result should not contain ANSI escape sequences
        assert!(!result.contains('\x1b'));
        // But should contain the diff content
        assert!(result.contains('-') || result.contains('+'));
    }

    #[test]
    fn format_colored_diff_normalizes_crlf_and_cr_terminators() {
        let bundle = compute_diff_with_theme("old\r\n", "new\r\n", DiffOptions::default());
        assert!(!bundle.formatted.contains("\r"));

        let bundle = compute_diff_with_theme("old\r", "new\r", DiffOptions::default());
        assert!(!bundle.formatted.contains("\r"));
        assert!(!bundle.formatted.contains("No newline at end"));
    }

    #[test]
    fn theme_names_round_trip() {
        for theme in DiffTheme::ALL {
            assert_eq!(DiffTheme::from_name(theme.name()), Some(theme));
        }
        assert_eq!(DiffTheme::from_name("unknown"), None);
    }

    #[test]
    fn every_theme_meets_wcag_aa_contrast() {
        fn relative_luminance((red, green, blue): (u8, u8, u8)) -> f64 {
            fn channel(value: u8) -> f64 {
                let scaled = f64::from(value) / 255.0;
                if scaled <= 0.039_28 {
                    scaled / 12.92
                } else {
                    ((scaled + 0.055) / 1.055).powf(2.4)
                }
            }
            0.212_6 * channel(red) + 0.715_2 * channel(green) + 0.072_2 * channel(blue)
        }

        fn contrast_ratio(foreground: (u8, u8, u8), background: (u8, u8, u8)) -> f64 {
            let foreground_luma = relative_luminance(foreground);
            let background_luma = relative_luminance(background);
            let (lighter, darker) = if foreground_luma >= background_luma {
                (foreground_luma, background_luma)
            } else {
                (background_luma, foreground_luma)
            };
            (lighter + 0.05) / (darker + 0.05)
        }

        for theme in DiffTheme::ALL {
            let palette = theme.palette();
            for (label, accent) in [
                ("header", palette.header),
                ("addition", palette.addition),
                ("deletion", palette.deletion),
            ] {
                let ratio = contrast_ratio(accent, palette.background);
                assert!(ratio >= 4.5, "{:?} {label} contrast {ratio:.2} is below WCAG AA 4.5:1", theme.name());
            }
        }
    }

    #[test]
    fn themed_rendering_uses_theme_colors() {
        let options = DiffOptions::default();
        let shared = shared_options(&options);
        let document = DiffDocument::between("old\n", "new\n", shared);
        let rendered = format_colored_diff_with_theme(&document.hunks, &options, DiffTheme::Dracula);
        // Dracula deletion accent (0xff, 0x55, 0x55) must appear in the output.
        assert!(rendered.contains("\x1b[38;2;255;85;85m"));
        // The default theme keeps the canonical portable ANSI-16 colors.
        let default_rendered = format_colored_diff(&document.hunks, &options);
        assert!(default_rendered.contains("\x1b[38;5;10m") || default_rendered.contains("\x1b[32m"));
    }
}
