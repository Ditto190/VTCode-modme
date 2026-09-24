//! Tool schema compaction utilities for session tool catalog.

use serde_json::{Value, json};
use vtcode_config::ToolDocumentationMode;

/// Default max description length for MCP tool descriptions.
/// MCP tools from external servers can have arbitrarily long descriptions;
/// capping them prevents token inflation. The cap applies in every mode and
/// never raises a mode's own limit.
pub const MCP_TOOL_DESCRIPTION_MAX_LEN: usize = 512;

/// Minimal mode sends only the first sentence of a tool description, capped
/// at this many characters, and strips parameter descriptions.
pub const MINIMAL_DESCRIPTION_MAX_CHARS: usize = 64;

/// Progressive mode (the default) keeps whole leading sentences of a tool
/// description up to this many characters. Builtin descriptions are limited
/// to 1500 characters by the description contract test and currently fit
/// well under this cap, so they reach the model intact; only unusually long
/// tails are dropped, at a sentence boundary.
pub const PROGRESSIVE_DESCRIPTION_MAX_CHARS: usize = 1024;

/// Progressive mode keeps parameter descriptions, trimming each one to whole
/// leading sentences within this many characters.
pub const PROGRESSIVE_PARAMETER_DESCRIPTION_MAX_CHARS: usize = 640;

/// Project a tool description for the given documentation mode.
///
/// - `Minimal`: first sentence without its terminal period, at most
///   [`MINIMAL_DESCRIPTION_MAX_CHARS`].
/// - `Progressive`: whole leading sentences within
///   [`PROGRESSIVE_DESCRIPTION_MAX_CHARS`].
/// - `Full`: the complete description.
///
/// `per_tool_max` (used for MCP tools) further lowers the limit in every mode.
/// Whitespace runs are collapsed to single spaces in all modes.
pub fn compact_tool_description(original: &str, mode: ToolDocumentationMode, per_tool_max: Option<usize>) -> String {
    // MCP tool descriptions arrive wrapped in host policy framing plus an
    // `<untrusted_mcp_description>` fence. Summarizing the raw text would
    // yield only the framing ("Host tool and permission policy remains
    // authoritative.") for every MCP tool, making deferred search results
    // indistinguishable. Strip the framing so the summary describes the tool;
    // full definitions keep the wrapper intact.
    let unframed = strip_mcp_policy_framing(original);
    let normalized = unframed.split_whitespace().collect::<Vec<_>>().join(" ");
    let limit = |mode_max: usize| per_tool_max.map_or(mode_max, |per_tool| per_tool.min(mode_max));

    match mode {
        ToolDocumentationMode::Minimal => {
            let first = first_sentence(&normalized);
            let first = first.strip_suffix('.').unwrap_or(first);
            truncate_chars_with_ellipsis(first, limit(MINIMAL_DESCRIPTION_MAX_CHARS))
        }
        ToolDocumentationMode::Progressive => {
            trim_to_leading_sentences(&normalized, limit(PROGRESSIVE_DESCRIPTION_MAX_CHARS))
        }
        ToolDocumentationMode::Full => trim_to_leading_sentences(&normalized, limit(usize::MAX)),
    }
}

/// Byte offsets just past each sentence terminator (`.`, `!`, `?`) that is
/// followed by the end of the text or by whitespace and a character that is
/// not lowercase. This keeps `.vtcode`, `llms.txt`, `0.5`, and `e.g. foo`
/// inside their sentence.
fn sentence_end_offsets(text: &str) -> impl Iterator<Item = usize> + '_ {
    text.char_indices().filter_map(move |(index, ch)| {
        if !matches!(ch, '.' | '!' | '?') {
            return None;
        }
        let end = index + ch.len_utf8();
        let rest = &text[end..];
        if rest.is_empty() {
            return Some(end);
        }
        if !rest.starts_with(char::is_whitespace) {
            return None;
        }
        match rest.trim_start().chars().next() {
            Some(following) if following.is_lowercase() => None,
            _ => Some(end),
        }
    })
}

fn first_sentence(text: &str) -> &str {
    sentence_end_offsets(text).next().map_or(text, |end| &text[..end])
}

/// Keep the longest run of whole leading sentences that fits in `max_chars`.
/// When even the first sentence is too long, cut it at a character boundary
/// and append an ellipsis.
fn trim_to_leading_sentences(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut kept = None;
    for end in sentence_end_offsets(text) {
        if text[..end].chars().count() > max_chars {
            break;
        }
        kept = Some(end);
    }
    match kept {
        Some(end) => text[..end].to_string(),
        None => truncate_chars_with_ellipsis(text, max_chars),
    }
}

fn truncate_chars_with_ellipsis(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let keep = max_chars.saturating_sub(1);
    let end = text.char_indices().nth(keep).map_or(text.len(), |(index, _)| index);
    format!("{}…", text[..end].trim_end())
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

/// Project a parameter schema for the given documentation mode.
///
/// `Full` returns the schema unchanged, `Progressive` keeps every parameter
/// description but trims long tails to
/// [`PROGRESSIVE_PARAMETER_DESCRIPTION_MAX_CHARS`], and `Minimal` removes
/// schema descriptions entirely.
pub fn compact_parameters(parameters: Value, mode: ToolDocumentationMode) -> Value {
    let mut compacted = parameters;
    match mode {
        ToolDocumentationMode::Full => {}
        ToolDocumentationMode::Progressive => {
            trim_schema_descriptions(&mut compacted, PROGRESSIVE_PARAMETER_DESCRIPTION_MAX_CHARS, false);
        }
        ToolDocumentationMode::Minimal => remove_schema_descriptions(&mut compacted),
    }
    compacted
}

fn trim_schema_descriptions(value: &mut Value, max_chars: usize, inside_properties_map: bool) {
    match value {
        Value::Object(map) => {
            if !inside_properties_map
                && let Some(Value::String(description)) = map.get_mut("description")
                && description.chars().count() > max_chars
            {
                *description = trim_to_leading_sentences(description, max_chars);
            }
            for (key, nested) in map.iter_mut() {
                trim_schema_descriptions(nested, max_chars, key == "properties");
            }
        }
        Value::Array(items) => {
            for item in items {
                trim_schema_descriptions(item, max_chars, false);
            }
        }
        _ => {}
    }
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
        let description = "Read a file from disk. Returns contents.";
        for mode in [ToolDocumentationMode::Progressive, ToolDocumentationMode::Full] {
            assert_eq!(compact_tool_description(description, mode, None), description);
        }
        assert_eq!(
            compact_tool_description(description, ToolDocumentationMode::Minimal, None),
            "Read a file from disk"
        );
    }

    #[test]
    fn progressive_description_keeps_later_sentences_and_trims_only_long_tails() {
        let description = "Run a shell command. For file edits, use apply_patch instead. Expanded modes need approval.";
        assert_eq!(compact_tool_description(description, ToolDocumentationMode::Progressive, None), description);

        let tail = " Extra detail sentence.".repeat(80);
        let long = format!("{description}{tail}");
        let trimmed = compact_tool_description(&long, ToolDocumentationMode::Progressive, None);
        assert!(trimmed.starts_with(description), "{trimmed}");
        assert!(trimmed.ends_with('.'), "trim must stop at a sentence boundary: {trimmed}");
        assert!(trimmed.chars().count() <= PROGRESSIVE_DESCRIPTION_MAX_CHARS);
    }

    #[test]
    fn sentence_boundaries_ignore_dots_inside_paths_and_abbreviations() {
        let description = "Plans live under .vtcode/plans/ and llms.txt, e.g. foo.md or 0.5 files. Second sentence.";
        assert_eq!(
            first_sentence(description),
            "Plans live under .vtcode/plans/ and llms.txt, e.g. foo.md or 0.5 files."
        );
        assert_eq!(first_sentence("No terminator here"), "No terminator here");
    }

    #[test]
    fn per_tool_max_never_raises_the_mode_limit() {
        let description = "A".repeat(200);
        let minimal =
            compact_tool_description(&description, ToolDocumentationMode::Minimal, Some(MCP_TOOL_DESCRIPTION_MAX_LEN));
        assert_eq!(minimal.chars().count(), MINIMAL_DESCRIPTION_MAX_CHARS);
        let full =
            compact_tool_description(&"B".repeat(900), ToolDocumentationMode::Full, Some(MCP_TOOL_DESCRIPTION_MAX_LEN));
        assert_eq!(full.chars().count(), MCP_TOOL_DESCRIPTION_MAX_LEN);
    }

    #[test]
    fn progressive_parameters_keep_descriptions_and_trim_long_tails() {
        let long_tail = " More detail.".repeat(80);
        let schema = json!({
            "type": "object",
            "description": "Top-level schema description.",
            "properties": {
                "cmd": {"type": "string", "description": "Command to run."},
                "mode": {"type": "string", "description": format!("Mode to use.{long_tail}")},
                "description": {"type": "string", "description": "A property literally named description."}
            }
        });

        let progressive = compact_parameters(schema.clone(), ToolDocumentationMode::Progressive);
        assert_eq!(progressive["properties"]["cmd"]["description"], json!("Command to run."));
        assert_eq!(
            progressive["properties"]["description"]["description"],
            json!("A property literally named description.")
        );
        let mode = progressive["properties"]["mode"]["description"]
            .as_str()
            .expect("mode description");
        assert!(mode.starts_with("Mode to use."));
        assert!(mode.chars().count() <= PROGRESSIVE_PARAMETER_DESCRIPTION_MAX_CHARS);

        let minimal = compact_parameters(schema.clone(), ToolDocumentationMode::Minimal);
        assert!(minimal["properties"]["cmd"].get("description").is_none());
        assert!(minimal["properties"]["description"].is_object());

        assert_eq!(compact_parameters(schema.clone(), ToolDocumentationMode::Full), schema);
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
