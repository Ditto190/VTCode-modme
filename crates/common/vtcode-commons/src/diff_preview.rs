//! Compatibility re-exports for `vtcode-diff` preview semantics.
//!
//! New internal consumers should depend on `vtcode-diff` directly. These
//! exports remain for one release so downstream callers can migrate.

pub use vtcode_diff::{
    DiffCell, DiffChangeCounts, DiffDisplayKind, DiffDisplayLine, DiffLayout, DiffRow, DiffRowKind, DiffSegment,
    IntralineRange, LayoutOptions, SideBySideRow, WordChangedRanges, annotate_word_level_diffs, count_diff_changes,
    diff_display_line_number_width, display_lines_from_hunks, display_lines_from_unified_diff,
    format_numbered_unified_diff, layout_display_lines, side_by_side_rows, word_level_changed_ranges,
};
