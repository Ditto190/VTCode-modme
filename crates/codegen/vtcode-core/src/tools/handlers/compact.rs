//! Tool schema compaction utilities for session tool catalog.

use serde_json::{Value, json};
use vtcode_config::ToolDocumentationMode;

/// Default max description length for MCP tool descriptions in Full mode.
/// MCP tools from external servers can have arbitrarily long descriptions;
/// capping them prevents token inflation.
pub const MCP_TOOL_DESCRIPTION_MAX_LEN: usize = 512;

pub fn compact_tool_description(original: &str, mode: ToolDocumentationMode, per_tool_max: Option<usize>) -> String {
    let mode_max = match mode {
        ToolDocumentationMode::Minimal => 64,
        ToolDocumentationMode::Progressive => 120,
        ToolDocumentationMode::Full => usize::MAX,
    };
    let max_len = per_tool_max.unwrap_or(mode_max);

    // MCP tool descriptions arrive wrapped in host policy framing plus an
    // `<untrusted_mcp_description>` fence. Summarizing the raw text would
    // yield only the framing ("Host tool and permission policy remains
    // authoritative.") for every MCP tool, making deferred search results
    // indistinguishable. Strip the framing so the summary describes the tool;
    // full definitions keep the wrapper intact.
    let unframed = strip_mcp_policy_framing(original);
    let sentence = unframed
        .split('.')
        .next()
        .unwrap_or(&unframed)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    if sentence.len() <= max_len {
        sentence
    } else {
        let target = max_len.saturating_sub(1);
        let end = sentence
            .char_indices()
            .map(|(index, _)| index)
            .rfind(|&index| index <= target)
            .unwrap_or(0);
        format!("{}…", &sentence[..end])
    }
}

/// Remove MCP host-policy framing and fence markup for summary purposes.
///
/// Returns the input unchanged when the framing is absent, so non-MCP
/// descriptions are never altered by this path.
fn strip_mcp_policy_framing(original: &str) -> String {
    use crate::tools::mcp::{MCP_POLICY_SENTENCE, MCP_UNTRUSTED_NOTE_SENTENCE};

    if !original.contains(MCP_POLICY_SENTENCE) {
        return original.to_string();
    }
    let without_policy = original
        .replace(MCP_POLICY_SENTENCE, "")
        .replace(MCP_UNTRUSTED_NOTE_SENTENCE, "");
    let mut lines: Vec<&str> = Vec::new();
    for line in without_policy.lines() {
        let trimmed = line.trim();
        // Drop the `<untrusted_mcp_description>` fence and its HTML comment;
        // the inner server-provided text is what the summary must convey.
        if trimmed.starts_with('<') {
            continue;
        }
        lines.push(line);
    }
    lines.join("\n")
}

pub fn compact_parameters(parameters: Value, mode: ToolDocumentationMode) -> Value {
    if matches!(mode, ToolDocumentationMode::Full) {
        return parameters;
    }

    let mut compacted = parameters;
    remove_schema_descriptions(&mut compacted);
    compacted
}

pub fn remove_schema_descriptions(value: &mut Value) {
    remove_schema_descriptions_impl(value, false);
}

fn remove_schema_descriptions_impl(value: &mut Value, inside_properties_map: bool) {
    match value {
        Value::Object(map) => {
            if !inside_properties_map {
                map.remove("description");
            }
            for (key, nested) in map.iter_mut() {
                remove_schema_descriptions_impl(nested, key == "properties");
            }
        }
        Value::Array(items) => {
            for item in items {
                remove_schema_descriptions_impl(item, false);
            }
        }
        _ => {}
    }
}

pub fn default_parameter_schema() -> Value {
    json!({
        "type": "object",
        "properties": {},
        "additionalProperties": true
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::mcp::{MCP_POLICY_SENTENCE, MCP_UNTRUSTED_NOTE_SENTENCE};

    fn wrapped_mcp_description(inner: &str) -> String {
        format!(
            "{MCP_POLICY_SENTENCE} {MCP_UNTRUSTED_NOTE_SENTENCE}\n<untrusted_mcp_description provider=\"deepwiki\" tool=\"ask_question\">\n<!-- comment -->\n{inner}\n</untrusted_mcp_description>\n{MCP_POLICY_SENTENCE}"
        )
    }

    #[test]
    fn compact_mcp_description_summarizes_inner_text_not_framing() {
        let summary = compact_tool_description(
            &wrapped_mcp_description("Ask a question about a repository. Returns an answer with citations."),
            ToolDocumentationMode::Progressive,
            None,
        );
        assert!(
            summary.starts_with("Ask a question about a repository"),
            "summary must describe the tool, got: {summary}"
        );
        assert!(!summary.contains("remains authoritative"));
        assert!(!summary.contains("untrusted_mcp_description"));
    }

    #[test]
    fn compact_plain_description_is_unchanged() {
        let summary = compact_tool_description(
            "Read a file from disk. Returns contents.",
            ToolDocumentationMode::Progressive,
            None,
        );
        assert_eq!(summary, "Read a file from disk");
    }

    #[test]
    fn compact_mcp_description_never_leaks_fence_markup() {
        let summary = compact_tool_description(
            &wrapped_mcp_description("Read the contents of a wiki. Returns markdown."),
            ToolDocumentationMode::Minimal,
            None,
        );
        assert!(!summary.contains("<untrusted"), "fence markup must not leak, got: {summary}");
        assert!(!summary.contains("<!--"), "fence comment must not leak, got: {summary}");
        assert!(summary.starts_with("Read the contents of a wiki"), "summary must describe the tool, got: {summary}");
    }
}
