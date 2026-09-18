use crate::tui::ui::syntax_highlight;
use anstyle::Style as AnsiStyle;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use vtcode_commons::diff_paths::{is_prose_language_hint, language_hint_from_path};
use vtcode_diff::{
    DiffDisplayKind, DiffDisplayLine, DiffDocument, DiffHunk as SharedDiffHunk, DiffOptions, display_lines_from_hunks,
};

type DiffLineKey = (DiffDisplayKind, Option<u32>, Option<u32>);
type DiffSyntaxSegments = Vec<(AnsiStyle, String)>;

const MAX_DIFF_SYNTAX_BYTES: usize = 512 * 1024;
const MAX_DIFF_SYNTAX_LINES: usize = 10_000;

/// A diff hunk representing a contiguous block of changes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffHunk {
    pub(crate) old_start: usize,
    pub(crate) new_start: usize,
    pub(crate) old_lines: usize,
    pub(crate) new_lines: usize,
    pub(crate) display: String,
}

impl DiffHunk {
    pub fn summary(&self) -> String {
        if self.old_lines > 0 && self.new_lines > 0 {
            format!("-{} +{}", self.old_lines, self.new_lines)
        } else if self.old_lines > 0 {
            format!("-{}", self.old_lines)
        } else if self.new_lines > 0 {
            format!("+{}", self.new_lines)
        } else {
            "No changes".to_string()
        }
    }

    pub fn old_position(&self) -> String {
        if self.old_lines == 0 {
            format!("{}", self.old_start + 1)
        } else {
            format!("{}-{}", self.old_start + 1, self.old_start + self.old_lines)
        }
    }

    pub fn new_position(&self) -> String {
        if self.new_lines == 0 {
            format!("{}", self.new_start + 1)
        } else {
            format!("{}-{}", self.new_start + 1, self.new_start + self.new_lines)
        }
    }
}

/// Trust mode for diff preview - how to handle file edit approval
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrustMode {
    Once,
    Session,
    Always,
    AutoTrust,
}

impl TrustMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Once => "Once",
            Self::Session => "Session",
            Self::Always => "Always",
            Self::AutoTrust => "AutoTrust",
        }
    }
}

/// Behavior mode for diff preview overlays.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiffPreviewMode {
    #[default]
    EditApproval,
    FileConflict,
    ReadonlyReview,
}

/// State for diff preview modal
#[derive(Debug, Clone)]
pub struct DiffPreviewState {
    pub(crate) file_path: String,
    pub(crate) before: String,
    pub(crate) after: String,
    hunks: Vec<DiffHunk>,
    pub(crate) current_hunk: usize,
    pub(crate) trust_mode: TrustMode,
    pub(crate) mode: DiffPreviewMode,
    pub(crate) document: DiffDocument,
    pub(crate) display_lines: Vec<DiffDisplayLine>,
    pub(crate) syntax_lines: HashMap<DiffLineKey, DiffSyntaxSegments>,
    pub(crate) scroll_offset: usize,
}

impl DiffPreviewState {
    pub(crate) fn new(file_path: String, before: String, after: String, hunks: Vec<DiffHunk>) -> Self {
        Self::new_with_mode(file_path, before, after, hunks, DiffPreviewMode::EditApproval)
    }

    pub(crate) fn new_with_mode(
        file_path: String,
        before: String,
        after: String,
        _hunks: Vec<DiffHunk>,
        mode: DiffPreviewMode,
    ) -> Self {
        let document = DiffDocument::between(&before, &after, DiffOptions::default());
        let display_lines = display_lines_from_hunks(&document.hunks);
        let syntax_lines = syntax_segments_for_hunks(&document.hunks, &file_path);
        let hunks = document
            .hunks
            .iter()
            .map(|hunk| DiffHunk {
                old_start: hunk.old_start.saturating_sub(1),
                new_start: hunk.new_start.saturating_sub(1),
                old_lines: hunk.old_lines,
                new_lines: hunk.new_lines,
                display: format!("@@ -{} +{} @@", hunk.old_start, hunk.new_start),
            })
            .collect();
        Self {
            file_path,
            before,
            after,
            hunks,
            current_hunk: 0,
            trust_mode: TrustMode::Once,
            mode,
            document,
            display_lines,
            syntax_lines,
            scroll_offset: 0,
        }
    }

    pub fn current_hunk_ref(&self) -> Option<&DiffHunk> {
        self.hunks.get(self.current_hunk)
    }

    pub(crate) fn hunk_count(&self) -> usize {
        self.document.hunks.len()
    }

    pub(crate) fn focus_hunk(&mut self, index: usize) {
        if index >= self.hunk_count() {
            return;
        }
        self.current_hunk = index;
        self.scroll_offset = self
            .display_lines
            .iter()
            .enumerate()
            .filter(|(_, line)| line.kind == DiffDisplayKind::HunkHeader)
            .nth(index)
            .map_or(0, |(row, _)| row);
    }

    pub(crate) fn scroll_by(&mut self, rows: isize) {
        self.scroll_offset = self.scroll_offset.saturating_add_signed(rows);
        self.scroll_offset = self.scroll_offset.min(self.display_lines.len().saturating_sub(1));
    }

    pub(crate) fn syntax_segments(
        &self,
        kind: DiffDisplayKind,
        old_line: Option<u32>,
        new_line: Option<u32>,
    ) -> Option<&[(AnsiStyle, String)]> {
        self.syntax_lines.get(&(kind, old_line, new_line)).map(Vec::as_slice)
    }
}

/// One reconstructed side (pre-image or post-image) of a diff, in original
/// file order, with the 1-based line number each joined source line maps to.
struct HunkSideSource {
    source: String,
    numbers: Vec<Option<u32>>,
}

/// Reconstructs one side of the diff in original file order so the
/// highlighter sees contiguous source instead of per-hunk fragments.
fn hunk_side_source(hunks: &[SharedDiffHunk], old_side: bool) -> HunkSideSource {
    let mut source = String::new();
    let mut numbers = Vec::new();
    for hunk in hunks {
        for line in &hunk.lines {
            let included = if old_side {
                line.kind != vtcode_diff::DiffLineKind::Addition
            } else {
                line.kind != vtcode_diff::DiffLineKind::Deletion
            };
            if !included {
                continue;
            }
            if !source.is_empty() {
                source.push('\n');
            }
            source.push_str(trim_line_ending(&line.text));
            numbers.push(if old_side { line.old_line } else { line.new_line });
        }
    }
    HunkSideSource { source, numbers }
}

fn syntax_segments_for_hunks(hunks: &[SharedDiffHunk], file_path: &str) -> HashMap<DiffLineKey, DiffSyntaxSegments> {
    let language = language_hint_from_path(file_path);
    if is_prose_language_hint(language.as_deref()) {
        return HashMap::new();
    }
    let theme = syntax_highlight::get_active_syntax_theme();

    // Codex-style: highlight each side of the file once, in original order,
    // so parser state survives hunk boundaries (multi-line strings and
    // comments split across hunks stay correctly highlighted).
    let old_side = hunk_side_source(hunks, true);
    let new_side = hunk_side_source(hunks, false);
    let within_budget = |side: &HunkSideSource| {
        side.source.len() <= MAX_DIFF_SYNTAX_BYTES && side.numbers.len() <= MAX_DIFF_SYNTAX_LINES
    };
    if within_budget(&old_side) && within_budget(&new_side) {
        let mut syntax_lines = HashMap::new();
        for (side, reconstruct_old) in [(&old_side, true), (&new_side, false)] {
            if side.source.is_empty() {
                continue;
            }
            let highlighted = syntax_highlight::highlight_code_to_anstyle_line_segments(
                &side.source,
                language.as_deref(),
                theme,
                true,
            );
            // Map segments back in the same zip order the source was built,
            // so each highlighted line lands on its exact hunk line without a
            // second pass or a number-keyed lookup (which could collide when
            // the same line number appears in repeated context).
            let mut included_lines = side.numbers.iter().zip(highlighted);
            for hunk in hunks {
                for line in &hunk.lines {
                    let included = if reconstruct_old {
                        line.kind != vtcode_diff::DiffLineKind::Addition
                    } else {
                        line.kind != vtcode_diff::DiffLineKind::Deletion
                    };
                    if !included {
                        continue;
                    }
                    if let Some((_, segments)) = included_lines.next() {
                        syntax_lines.insert((diff_display_kind(line.kind), line.old_line, line.new_line), segments);
                    }
                }
            }
        }
        return syntax_lines;
    }

    // Oversized: fall back to per-hunk highlighting under the shared budget.
    let mut syntax_lines = HashMap::new();
    let mut remaining_bytes = MAX_DIFF_SYNTAX_BYTES;
    let mut remaining_lines = MAX_DIFF_SYNTAX_LINES;
    for hunk in hunks {
        let mut source = String::new();
        for (index, line) in hunk.lines.iter().enumerate() {
            source.push_str(trim_line_ending(&line.text));
            if index + 1 < hunk.lines.len() {
                source.push('\n');
            }
        }
        if source.len() > remaining_bytes || hunk.lines.len() > remaining_lines {
            break;
        }
        remaining_bytes = remaining_bytes.saturating_sub(source.len());
        remaining_lines = remaining_lines.saturating_sub(hunk.lines.len());
        let highlighted =
            syntax_highlight::highlight_code_to_anstyle_line_segments(&source, language.as_deref(), theme, true);
        for (line, segments) in hunk.lines.iter().zip(highlighted) {
            syntax_lines.insert((diff_display_kind(line.kind), line.old_line, line.new_line), segments);
        }
    }
    syntax_lines
}

fn diff_display_kind(kind: vtcode_diff::DiffLineKind) -> DiffDisplayKind {
    match kind {
        vtcode_diff::DiffLineKind::Context => DiffDisplayKind::Context,
        vtcode_diff::DiffLineKind::Addition => DiffDisplayKind::Addition,
        vtcode_diff::DiffLineKind::Deletion => DiffDisplayKind::Deletion,
    }
}

fn trim_line_ending(text: &str) -> &str {
    text.strip_suffix("\r\n")
        .or_else(|| text.strip_suffix('\n'))
        .or_else(|| text.strip_suffix('\r'))
        .unwrap_or(text)
}
