//! Compatibility re-exports for the extracted `vtcode-diff` crate.
//!
//! New internal consumers should depend on `vtcode-diff` directly. These
//! exports remain for one release so downstream callers can migrate.

pub use vtcode_diff::{
    Chunk, DiffAlgorithm, DiffBundle, DiffDocument, DiffHunk, DiffLine, DiffLineKind, DiffStats, ParseDiffError,
    compute_diff_chunks,
};

/// Options retained for the legacy `vtcode_commons::diff` API.
#[derive(Debug, Clone)]
pub struct DiffOptions<'a> {
    pub context_lines: usize,
    pub old_label: Option<&'a str>,
    pub new_label: Option<&'a str>,
    pub missing_newline_hint: bool,
}

impl Default for DiffOptions<'_> {
    fn default() -> Self {
        Self {
            context_lines: 3,
            old_label: None,
            new_label: None,
            missing_newline_hint: true,
        }
    }
}

/// Compute a diff through the extracted implementation while preserving the legacy options type.
pub fn compute_diff<F>(old: &str, new: &str, options: DiffOptions<'_>, formatter: F) -> DiffBundle
where
    F: FnOnce(&[DiffHunk], &DiffOptions<'_>) -> String,
{
    let extracted_options = vtcode_diff::DiffOptions {
        context_lines: options.context_lines,
        old_label: options.old_label,
        new_label: options.new_label,
        missing_newline_hint: options.missing_newline_hint,
        ..vtcode_diff::DiffOptions::default()
    };

    vtcode_diff::compute_diff(old, new, extracted_options, |hunks, _| formatter(hunks, &options))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_options_struct_literals_remain_supported() {
        let result = compute_diff(
            "old\n",
            "new\n",
            DiffOptions {
                context_lines: 0,
                old_label: None,
                new_label: None,
                missing_newline_hint: true,
            },
            |hunks, options| {
                assert_eq!(options.context_lines, 0);
                hunks.len().to_string()
            },
        );

        assert_eq!(result.formatted, "1");
    }
}
