//! Unified diff formatting with ANSI colors.
//!
//! Provides `format_colored_diff` as the single canonical implementation
//! for rendering diff hunks with terminal colors. Previously duplicated
//! in `vtcode-core` and `vtcode-ui`.

use anstyle::{AnsiColor, Color, Reset, Style};
use std::fmt::Write;
use vtcode_diff::{DiffDocument, format_unified_hunks};

// Keep the published UI facade's legacy options shape while exposing the
// renderer-neutral types from the extracted implementation. New consumers
// that need algorithm or timeout controls should depend on `vtcode-diff`.
pub use vtcode_commons::diff::{DiffOptions, compute_diff};
pub use vtcode_diff::{Chunk, DiffBundle, DiffHunk, DiffLine, DiffLineKind, compute_diff_chunks};

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

    let cyan_style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Cyan)));
    let addition_style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Green)));
    let deletion_style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Red)));
    let context_style = Style::new();

    let mut output = String::new();

    if let (Some(old_label), Some(new_label)) = (options.old_label, options.new_label) {
        let _ = write!(output, "{}--- {old_label}\n{}", cyan_style.render(), Reset.render());

        let _ = write!(output, "{}+++ {new_label}\n{}", cyan_style.render(), Reset.render());
    }

    for hunk in hunks {
        let _ = write!(
            output,
            "{}@@ -{},{} +{},{} @@\n{}",
            cyan_style.render(),
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
}
