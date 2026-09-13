#![expect(
    clippy::string_slice,
    clippy::cast_possible_truncation,
    clippy::indexing_slicing,
    reason = "Word-level LCS walks atom indices by design; offsets are bounded to the source line."
)]

//! Shared helpers for rendering diff previews.
//!
//! Layout: `{sign}{line_no} │ content` (single gutter) with two-level
//! backgrounds — full-width add/del tint plus stronger word-level chips on
//! tokens that differ from the paired opposite line.

use crate::diff::{DiffHunk, DiffLineKind};
use crate::diff_paths::{
    format_start_only_hunk_header, is_diff_addition_line, is_diff_deletion_line, parse_hunk_starts,
};

/// Intra-line highlight: list of `(start, end)` byte ranges in a line body.
pub type WordChangedRanges = Vec<(usize, usize)>;

/// Cap atoms so the LCS DP stays O(n·m) with a hard memory bound on huge lines.
const MAX_WORD_ATOMS: usize = 512;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DiffChangeCounts {
    pub additions: usize,
    pub deletions: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiffDisplayKind {
    Metadata,
    HunkHeader,
    Context,
    Addition,
    Deletion,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiffDisplayLine {
    pub kind: DiffDisplayKind,
    /// Line number in the old file (context/deletion), when known.
    pub old_line: Option<u32>,
    /// Line number in the new file (context/addition), when known.
    pub new_line: Option<u32>,
    pub text: String,
    /// Byte ranges inside `text` that differ from the paired opposite line
    /// (word-level / intra-line highlight). Empty when no pair or no overlap.
    pub changed: WordChangedRanges,
}

impl DiffDisplayLine {
    pub fn body(kind: DiffDisplayKind, old_line: Option<u32>, new_line: Option<u32>, text: String) -> Self {
        Self {
            kind,
            old_line,
            new_line,
            text,
            changed: Vec::new(),
        }
    }

    /// Whether this line carries diff content, without re-parsing its text.
    pub fn is_diff(&self) -> bool {
        self.kind.is_diff()
    }

    /// Single-gutter form: `sign + number + │ + content`.
    ///
    /// Deletions show the old number, additions the new, context the new
    /// (falling back to old). The `│` keeps markdown bullets (`- foo`)
    /// distinct from the diff marker.
    pub fn numbered_text(&self, line_number_width: usize) -> String {
        let w = line_number_width;
        match self.kind {
            DiffDisplayKind::Metadata | DiffDisplayKind::HunkHeader => self.text.clone(),
            DiffDisplayKind::Deletion => {
                format!("-{:>w$} │ {}", self.old_line.unwrap_or_default(), self.text)
            }
            DiffDisplayKind::Addition => {
                format!("+{:>w$} │ {}", self.new_line.unwrap_or_default(), self.text)
            }
            DiffDisplayKind::Context => {
                let no = self.new_line.or(self.old_line).unwrap_or_default();
                format!(" {:>w$} │ {}", no, self.text)
            }
        }
    }
}

impl DiffDisplayKind {
    /// Whether this kind carries diff body content (context, addition, or
    /// deletion) rather than metadata or a hunk header.
    pub fn is_diff(self) -> bool {
        matches!(self, Self::Context | Self::Addition | Self::Deletion)
    }
}

impl DiffChangeCounts {
    pub fn total(self) -> usize {
        self.additions + self.deletions
    }
}

pub fn count_diff_changes(hunks: &[DiffHunk]) -> DiffChangeCounts {
    let mut counts = DiffChangeCounts::default();

    for hunk in hunks {
        for line in &hunk.lines {
            match line.kind {
                DiffLineKind::Addition => counts.additions += 1,
                DiffLineKind::Deletion => counts.deletions += 1,
                DiffLineKind::Context => {}
            }
        }
    }

    counts
}

pub fn display_lines_from_hunks(hunks: &[DiffHunk]) -> Vec<DiffDisplayLine> {
    // Each hunk contributes 1 header + its lines; pre-size to avoid reallocations
    // on large diffs (the count is exact, so no over-allocation).
    let total = hunks.iter().map(|h| 1 + h.lines.len()).sum();
    let mut lines = Vec::with_capacity(total);

    for hunk in hunks {
        lines.push(DiffDisplayLine::body(
            DiffDisplayKind::HunkHeader,
            None,
            None,
            format!("@@ -{} +{} @@", hunk.old_start, hunk.new_start),
        ));

        for line in &hunk.lines {
            lines.push(display_line_from_diff_line(line));
        }
    }

    annotate_word_level_diffs(&mut lines);
    lines
}

/// One rendered row in a side-by-side diff view.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SideBySideRow {
    /// Full-width band (hunk header / metadata) stored on `left`; otherwise
    /// the old-file pane. `None` means an empty left cell.
    pub left: Option<DiffDisplayLine>,
    /// New-file pane. `None` means an empty right cell.
    pub right: Option<DiffDisplayLine>,
}

impl SideBySideRow {
    /// True when this row is a full-width band rather than a split pair.
    pub fn is_full_width(&self) -> bool {
        self.left
            .as_ref()
            .is_some_and(|l| matches!(l.kind, DiffDisplayKind::HunkHeader | DiffDisplayKind::Metadata))
            && self.right.is_none()
    }
}

/// Pair display lines into side-by-side rows.
///
/// Context lines appear on both sides. Consecutive deletion/addition runs are
/// zipped index-wise so old and new lines sit on the same visual row. Hunk
/// headers and metadata span the full width (stored on `left`).
pub fn side_by_side_rows(lines: &[DiffDisplayLine]) -> Vec<SideBySideRow> {
    let mut rows = Vec::with_capacity(lines.len());
    let mut i = 0usize;

    while i < lines.len() {
        match lines[i].kind {
            DiffDisplayKind::HunkHeader | DiffDisplayKind::Metadata => {
                rows.push(SideBySideRow { left: Some(lines[i].clone()), right: None });
                i += 1;
            }
            DiffDisplayKind::Context => {
                rows.push(SideBySideRow {
                    left: Some(lines[i].clone()),
                    right: Some(lines[i].clone()),
                });
                i += 1;
            }
            DiffDisplayKind::Deletion => {
                let del_start = i;
                while i < lines.len() && lines[i].kind == DiffDisplayKind::Deletion {
                    i += 1;
                }
                let dels = &lines[del_start..i];
                let add_start = i;
                while i < lines.len() && lines[i].kind == DiffDisplayKind::Addition {
                    i += 1;
                }
                let adds = &lines[add_start..i];
                let pairs = dels.len().max(adds.len());
                for pair in 0..pairs {
                    rows.push(SideBySideRow {
                        left: dels.get(pair).cloned(),
                        right: adds.get(pair).cloned(),
                    });
                }
            }
            DiffDisplayKind::Addition => {
                let add_start = i;
                while i < lines.len() && lines[i].kind == DiffDisplayKind::Addition {
                    i += 1;
                }
                for line in &lines[add_start..i] {
                    rows.push(SideBySideRow { left: None, right: Some(line.clone()) });
                }
            }
        }
    }

    rows
}

pub fn display_lines_from_unified_diff(diff_content: &str) -> Vec<DiffDisplayLine> {
    // Upper-bound the capacity to the line count — the double scan is cheap
    // (L1-bound byte search) and avoids 10+ reallocations on large diffs.
    let mut lines = Vec::with_capacity(diff_content.lines().count());
    let mut old_line_no = 0u32;
    let mut new_line_no = 0u32;
    let mut in_hunk = false;

    for line in diff_content.lines() {
        if let Some((old_start, new_start)) = parse_hunk_starts(line) {
            old_line_no = old_start as u32;
            new_line_no = new_start as u32;
            in_hunk = true;
            lines.push(DiffDisplayLine::body(
                DiffDisplayKind::HunkHeader,
                None,
                None,
                format_start_only_hunk_header(line).unwrap_or_else(|| format!("@@ -{old_start} +{new_start} @@")),
            ));
            continue;
        }

        if !in_hunk {
            lines.push(DiffDisplayLine::body(DiffDisplayKind::Metadata, None, None, line.to_string()));
            continue;
        }

        if is_diff_addition_line(line) {
            lines.push(DiffDisplayLine::body(
                DiffDisplayKind::Addition,
                None,
                Some(new_line_no),
                line[1..].to_string(),
            ));
            new_line_no = new_line_no.saturating_add(1);
            continue;
        }

        if is_diff_deletion_line(line) {
            lines.push(DiffDisplayLine::body(
                DiffDisplayKind::Deletion,
                Some(old_line_no),
                None,
                line[1..].to_string(),
            ));
            old_line_no = old_line_no.saturating_add(1);
            continue;
        }

        if let Some(context_line) = line.strip_prefix(' ') {
            lines.push(DiffDisplayLine::body(
                DiffDisplayKind::Context,
                Some(old_line_no),
                Some(new_line_no),
                context_line.to_string(),
            ));
            old_line_no = old_line_no.saturating_add(1);
            new_line_no = new_line_no.saturating_add(1);
            continue;
        }

        if let Some(omitted) = parse_omitted_line_count(line) {
            old_line_no = old_line_no.saturating_add(omitted);
            new_line_no = new_line_no.saturating_add(omitted);
            lines.push(DiffDisplayLine::body(DiffDisplayKind::Metadata, None, None, line.to_string()));
            continue;
        }

        lines.push(DiffDisplayLine::body(DiffDisplayKind::Metadata, None, None, line.to_string()));
    }

    annotate_word_level_diffs(&mut lines);
    lines
}

pub fn diff_display_line_number_width(lines: &[DiffDisplayLine]) -> usize {
    let max_digits = lines
        .iter()
        .flat_map(|line| [line.old_line, line.new_line])
        .flatten()
        .map(digit_count)
        .max()
        .unwrap_or(4);
    max_digits.clamp(5, 6)
}

fn digit_count(mut value: u32) -> usize {
    let mut digits = 1;
    while value >= 10 {
        value /= 10;
        digits += 1;
    }
    digits
}

pub fn format_numbered_unified_diff(diff_content: &str) -> Vec<String> {
    let display_lines = display_lines_from_unified_diff(diff_content);
    let width = diff_display_line_number_width(&display_lines);
    display_lines.into_iter().map(|line| line.numbered_text(width)).collect()
}

/// Parse the number of omitted lines from a condensation marker such as
/// `"... 12 lines omitted ..."`.
fn parse_omitted_line_count(line: &str) -> Option<u32> {
    let trimmed = line.trim();
    let after = trimmed.strip_prefix("...")?;
    let after = after.trim_start();
    let digits_end = after.find(|ch: char| !ch.is_ascii_digit())?;
    if digits_end == 0 {
        return None;
    }
    after[..digits_end].parse().ok()
}

fn display_line_from_diff_line(line: &crate::diff::DiffLine) -> DiffDisplayLine {
    let text = line.text.trim_end_matches('\n').to_string();
    match line.kind {
        DiffLineKind::Context => DiffDisplayLine::body(DiffDisplayKind::Context, line.old_line, line.new_line, text),
        DiffLineKind::Addition => DiffDisplayLine::body(DiffDisplayKind::Addition, line.old_line, line.new_line, text),
        DiffLineKind::Deletion => DiffDisplayLine::body(DiffDisplayKind::Deletion, line.old_line, line.new_line, text),
    }
}

/// Annotate word-level (intra-line) changed ranges on consecutive `-`/`+` pairs.
///
/// Groups consecutive Deletion lines followed by consecutive Addition lines,
/// pairs them index-wise, and stores byte ranges of tokens that differ. This
/// powers the two-level background: full-width line tint + stronger word chips.
pub fn annotate_word_level_diffs(lines: &mut [DiffDisplayLine]) {
    let mut i = 0;
    while i < lines.len() {
        if lines[i].kind != DiffDisplayKind::Deletion {
            i += 1;
            continue;
        }
        let del_start = i;
        while i < lines.len() && lines[i].kind == DiffDisplayKind::Deletion {
            i += 1;
        }
        let del_end = i;
        let add_start = i;
        while i < lines.len() && lines[i].kind == DiffDisplayKind::Addition {
            i += 1;
        }
        let add_end = i;
        let (before, rest) = lines.split_at_mut(add_start);
        let dels = &mut before[del_start..del_end];
        let adds = &mut rest[..(add_end - add_start)];
        pair_word_level_ranges(dels, adds);
    }
}

fn pair_word_level_ranges(dels: &mut [DiffDisplayLine], adds: &mut [DiffDisplayLine]) {
    let pairs = dels.len().min(adds.len());
    let mut computed = Vec::with_capacity(pairs);
    for idx in 0..pairs {
        computed.push(word_level_changed_ranges(&dels[idx].text, &adds[idx].text));
    }
    for (idx, (old_ranges, new_ranges)) in computed.into_iter().enumerate() {
        dels[idx].changed = old_ranges;
        adds[idx].changed = new_ranges;
    }
}

/// Minimum shared-token ratio before word chips are useful.
///
/// Below this, the pair is effectively a replace of whole lines — chips would
/// paint nearly every token and drown the clean full-width line tint.
const MIN_WORD_SIMILARITY: f64 = 0.35;

/// Split into word-ish atoms: identifier runs, whitespace runs, single other chars.
fn tokenize_atoms(text: &str) -> Vec<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut spans = Vec::with_capacity(text.len() / 2 + 1);
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        let start = i;
        if b.is_ascii_whitespace() {
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
        } else if b.is_ascii_alphanumeric() || b == b'_' {
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
        } else {
            let ch_len = text[i..].chars().next().map(char::len_utf8).unwrap_or(1);
            i += ch_len;
        }
        if i > start {
            spans.push((start, i));
        }
    }
    spans
}

/// Compute byte ranges in `old`/`new` that are not part of a common token subsequence.
///
/// Returns empty ranges when the lines are too dissimilar (or one side is
/// blank) so pure inserts/deletes keep a clean full-width tint without chips.
pub fn word_level_changed_ranges(old: &str, new: &str) -> (WordChangedRanges, WordChangedRanges) {
    let old_atoms = tokenize_atoms(old);
    let new_atoms = tokenize_atoms(new);
    if old_atoms.is_empty() || new_atoms.is_empty() {
        return (WordChangedRanges::new(), WordChangedRanges::new());
    }
    // Pathological long lines: skip chips rather than run a huge LCS DP.
    if old_atoms.len() > MAX_WORD_ATOMS || new_atoms.len() > MAX_WORD_ATOMS {
        return (WordChangedRanges::new(), WordChangedRanges::new());
    }

    let n = old_atoms.len();
    let m = new_atoms.len();
    let mut dp = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            let old_tok = &old[old_atoms[i].0..old_atoms[i].1];
            let new_tok = &new[new_atoms[j].0..new_atoms[j].1];
            dp[i][j] = if old_tok == new_tok {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }

    let lcs = dp[0][0];
    let max_len = n.max(m);
    let similarity = if max_len == 0 { 1.0 } else { lcs as f64 / max_len as f64 };
    if similarity < MIN_WORD_SIMILARITY {
        return (WordChangedRanges::new(), WordChangedRanges::new());
    }

    let mut old_changed = vec![false; n];
    let mut new_changed = vec![false; m];
    let (mut i, mut j) = (0usize, 0usize);
    while i < n && j < m {
        let old_tok = &old[old_atoms[i].0..old_atoms[i].1];
        let new_tok = &new[new_atoms[j].0..new_atoms[j].1];
        if old_tok == new_tok {
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            old_changed[i] = true;
            i += 1;
        } else {
            new_changed[j] = true;
            j += 1;
        }
    }
    while i < n {
        old_changed[i] = true;
        i += 1;
    }
    while j < m {
        new_changed[j] = true;
        j += 1;
    }

    let old_changed_count = old_changed.iter().filter(|&&f| f).count();
    let new_changed_count = new_changed.iter().filter(|&&f| f).count();
    if mostly_changed(old_changed_count, n) || mostly_changed(new_changed_count, m) {
        return (WordChangedRanges::new(), WordChangedRanges::new());
    }

    (
        collapse_atom_flags(&old_atoms, &old_changed, old),
        collapse_atom_flags(&new_atoms, &new_changed, new),
    )
}

/// True when more than half the atoms on a side are marked changed.
fn mostly_changed(changed_atoms: usize, atom_count: usize) -> bool {
    atom_count > 0 && changed_atoms * 2 > atom_count
}

fn collapse_atom_flags(atoms: &[(usize, usize)], flags: &[bool], text: &str) -> Vec<(usize, usize)> {
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for (idx, (start, end)) in atoms.iter().enumerate() {
        if !flags[idx] {
            continue;
        }
        match ranges.last_mut() {
            Some(last) if last.1 == *start => last.1 = *end,
            _ => ranges.push((*start, *end)),
        }
    }
    ranges.retain(|(start, end)| text[*start..*end].chars().any(|c| !c.is_ascii_whitespace()));
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::{DiffLine, DiffLineKind};

    #[test]
    fn counts_diff_changes_from_hunks() {
        let hunks = vec![DiffHunk {
            old_start: 1,
            old_lines: 2,
            new_start: 1,
            new_lines: 2,
            lines: vec![
                DiffLine {
                    kind: DiffLineKind::Context,
                    old_line: Some(1),
                    new_line: Some(1),
                    text: "same\n".to_string(),
                },
                DiffLine {
                    kind: DiffLineKind::Deletion,
                    old_line: Some(2),
                    new_line: None,
                    text: "old\n".to_string(),
                },
                DiffLine {
                    kind: DiffLineKind::Addition,
                    old_line: None,
                    new_line: Some(2),
                    text: "new\n".to_string(),
                },
            ],
        }];

        let counts = count_diff_changes(&hunks);
        assert_eq!(counts.additions, 1);
        assert_eq!(counts.deletions, 1);
        assert_eq!(counts.total(), 2);
    }

    #[test]
    fn is_diff_discriminates_content_lines() {
        assert!(DiffDisplayKind::Context.is_diff());
        assert!(DiffDisplayKind::Addition.is_diff());
        assert!(DiffDisplayKind::Deletion.is_diff());
        assert!(!DiffDisplayKind::Metadata.is_diff());
        assert!(!DiffDisplayKind::HunkHeader.is_diff());
    }

    #[test]
    fn formats_numbered_unified_diff_with_single_gutter() {
        let diff = "\
@@ -10,2 +10,2 @@
 old
-old
+new
";
        let lines = format_numbered_unified_diff(diff);
        assert!(lines.iter().any(|line| line == "@@ -10 +10 @@"));
        // Context: blank sign + new number (width clamps to 5).
        assert!(lines.iter().any(|line| line.contains("    10 │ old")));
        // Deletion: old number.
        assert!(lines.iter().any(|line| line.contains("-   11 │ old")));
        // Addition: new number.
        assert!(lines.iter().any(|line| line.contains("+   11 │ new")));
    }

    #[test]
    fn numbered_text_uses_pipe_separator_for_markdown_bullets() {
        let line = DiffDisplayLine::body(
            DiffDisplayKind::Addition,
            None,
            Some(53),
            "- **Agent-first by design*: prose".to_string(),
        );
        let text = line.numbered_text(5);
        assert_eq!(text, "+   53 │ - **Agent-first by design*: prose");
    }

    #[test]
    fn display_lines_from_hunks_preserves_semantics() {
        let hunks = vec![DiffHunk {
            old_start: 10,
            old_lines: 2,
            new_start: 10,
            new_lines: 2,
            lines: vec![
                DiffLine {
                    kind: DiffLineKind::Deletion,
                    old_line: Some(10),
                    new_line: None,
                    text: "old\n".to_string(),
                },
                DiffLine {
                    kind: DiffLineKind::Addition,
                    old_line: None,
                    new_line: Some(10),
                    text: "new\n".to_string(),
                },
                DiffLine {
                    kind: DiffLineKind::Context,
                    old_line: Some(11),
                    new_line: Some(11),
                    text: "same\n".to_string(),
                },
            ],
        }];

        let lines = display_lines_from_hunks(&hunks);
        assert_eq!(lines[0].kind, DiffDisplayKind::HunkHeader);
        assert_eq!(lines[0].text, "@@ -10 +10 @@");
        assert_eq!(lines[1].kind, DiffDisplayKind::Deletion);
        assert_eq!(lines[1].old_line, Some(10));
        assert_eq!(lines[1].new_line, None);
        assert_eq!(lines[2].kind, DiffDisplayKind::Addition);
        assert_eq!(lines[2].old_line, None);
        assert_eq!(lines[2].new_line, Some(10));
        assert_eq!(lines[3].kind, DiffDisplayKind::Context);
        assert_eq!(lines[3].old_line, Some(11));
        assert_eq!(lines[3].new_line, Some(11));
    }

    #[test]
    fn metadata_lines_stay_metadata_after_hunk() {
        let diff = "\
@@ -1 +1 @@
-old
\\ No newline at end of file
+new
";

        let lines = display_lines_from_unified_diff(diff);
        assert_eq!(lines[2].kind, DiffDisplayKind::Metadata);
        assert_eq!(lines[3].kind, DiffDisplayKind::Addition);
        assert_eq!(lines[3].new_line, Some(1));
    }

    #[test]
    fn diff_display_line_number_width_tracks_max_digits() {
        let lines = vec![
            DiffDisplayLine::body(DiffDisplayKind::Addition, None, Some(99), "let a = 1;".to_string()),
            DiffDisplayLine::body(DiffDisplayKind::Context, Some(1), Some(10_420), "let b = 2;".to_string()),
        ];

        assert_eq!(diff_display_line_number_width(&lines), 5);
    }

    #[test]
    fn diff_display_line_number_width_clamps_to_bounds() {
        let small = vec![DiffDisplayLine::body(
            DiffDisplayKind::Context,
            Some(1),
            Some(1),
            "text".to_string(),
        )];
        assert_eq!(diff_display_line_number_width(&small), 5);

        let large = vec![DiffDisplayLine::body(
            DiffDisplayKind::Context,
            Some(100_000),
            Some(100_000),
            "text".to_string(),
        )];
        assert_eq!(diff_display_line_number_width(&large), 6);
    }

    #[test]
    fn word_level_diff_highlights_only_changed_tokens() {
        let old = "let bright_red = anstyle::Color::Ansi(anstyle::AnsiColor::BrightRed);";
        let new = "let bright_red = anstyle::Color::Rgb(anstyle::RgbColor(255, 90, 90));";
        let (old_ranges, new_ranges) = word_level_changed_ranges(old, new);
        assert!(!old_ranges.is_empty());
        assert!(!new_ranges.is_empty());
        let old_changed: String = old_ranges.iter().map(|&(s, e)| &old[s..e]).collect();
        let new_changed: String = new_ranges.iter().map(|&(s, e)| &new[s..e]).collect();
        assert!(old_changed.contains("Ansi"));
        assert!(new_changed.contains("Rgb"));
        assert!(!old_changed.contains("bright_red"));
        assert!(!new_changed.contains("bright_red"));
    }

    #[test]
    fn word_level_diff_skips_dissimilar_pairs() {
        let old = "| Pillar | What it means |\n| --- | --- |\n| **Harness** | The model reasons; the harness composes tools. |";
        let new = "- **The loop is the product.** Tool composition, context management, and\n  verification are engineered — not improvised around a chat completion.";
        let (old_ranges, new_ranges) = word_level_changed_ranges(old, new);
        assert!(old_ranges.is_empty(), "dissimilar del pair should stay line-only");
        assert!(new_ranges.is_empty(), "dissimilar add pair should stay line-only");
    }

    #[test]
    fn word_level_diff_skips_blank_sides() {
        let (old_ranges, new_ranges) = word_level_changed_ranges("moved block", "");
        assert!(old_ranges.is_empty());
        assert!(new_ranges.is_empty());
    }

    #[test]
    fn word_level_diff_skips_oversized_atom_lists() {
        let long_a = "a ".repeat(MAX_WORD_ATOMS + 10);
        let long_b = "b ".repeat(MAX_WORD_ATOMS + 10);
        let (old_ranges, new_ranges) = word_level_changed_ranges(&long_a, &long_b);
        assert!(old_ranges.is_empty());
        assert!(new_ranges.is_empty());
    }

    #[test]
    fn side_by_side_pairs_context_on_both_columns() {
        let lines = display_lines_from_hunks(&[DiffHunk {
            old_start: 1,
            old_lines: 1,
            new_start: 1,
            new_lines: 1,
            lines: vec![DiffLine {
                kind: DiffLineKind::Context,
                old_line: Some(1),
                new_line: Some(1),
                text: "same\n".to_string(),
            }],
        }]);
        let rows = side_by_side_rows(&lines);
        assert_eq!(rows.len(), 2);
        let context = &rows[1];
        assert_eq!(context.left.as_ref().map(|l| l.text.as_str()), Some("same"));
        assert_eq!(context.right.as_ref().map(|l| l.text.as_str()), Some("same"));
    }

    #[test]
    fn side_by_side_zips_del_add_runs() {
        let lines = display_lines_from_unified_diff(
            "\
@@ -1,2 +1,2 @@
-old a
-old b
+new a
+new b
",
        );
        let rows = side_by_side_rows(&lines);
        assert!(rows[0].is_full_width());
        assert_eq!(rows[1].left.as_ref().map(|l| l.text.as_str()), Some("old a"));
        assert_eq!(rows[1].right.as_ref().map(|l| l.text.as_str()), Some("new a"));
        assert_eq!(rows[2].left.as_ref().map(|l| l.text.as_str()), Some("old b"));
        assert_eq!(rows[2].right.as_ref().map(|l| l.text.as_str()), Some("new b"));
    }

    #[test]
    fn side_by_side_keeps_unpaired_additions_on_right_only() {
        let lines = display_lines_from_unified_diff(
            "\
@@ -1 +1,2 @@
 keep
+added
",
        );
        let rows = side_by_side_rows(&lines);
        assert!(
            rows.iter()
                .any(|row| row.left.is_none() && row.right.as_ref().is_some_and(|r| r.text == "added"))
        );
    }

    #[test]
    fn side_by_side_keeps_unpaired_deletions_on_left_only() {
        let lines = display_lines_from_unified_diff(
            "\
@@ -2,1 +1 @@
 keep
-removed
",
        );
        let rows = side_by_side_rows(&lines);
        assert!(
            rows.iter()
                .any(|row| row.right.is_none() && row.left.as_ref().is_some_and(|l| l.text == "removed"))
        );
    }

    #[test]
    fn annotate_pairs_consecutive_del_add_runs() {
        let diff = "\
@@ -1 +1 @@
-let a = 1;
+let a = 2;
";
        let lines = display_lines_from_unified_diff(diff);
        let del = lines.iter().find(|l| l.kind == DiffDisplayKind::Deletion).unwrap();
        let add = lines.iter().find(|l| l.kind == DiffDisplayKind::Addition).unwrap();
        assert!(!del.changed.is_empty());
        assert!(!add.changed.is_empty());
        assert_eq!(&del.text[del.changed[0].0..del.changed[0].1], "1");
        assert_eq!(&add.text[add.changed[0].0..add.changed[0].1], "2");
    }

    #[test]
    fn word_chip_user_example_pair() {
        let old = "wrong, these are the defaults you were missing.";
        let new = "wrong, these are the defaults you were missing — built in, not bolted on.";
        let (old_ranges, new_ranges) = word_level_changed_ranges(old, new);
        assert!(old_ranges.is_empty(), "shared prefix should stay line-only");
        assert!(!new_ranges.is_empty(), "addition suffix should be a chip");
        let new_changed: String = new_ranges.iter().map(|&(s, e)| &new[s..e]).collect();
        assert!(new_changed.contains("built"), "got {new_changed:?}");
    }
}
