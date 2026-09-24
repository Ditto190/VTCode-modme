use crate::provider::{LLMRequest, MessageRole};
use crate::providers::anthropic_types::CacheControl;
use crate::providers::shared::split_dynamic_prompt_suffix;
use serde_json::{Value, json};

pub(crate) struct SystemPromptBuildResult {
    pub system_value: Option<Value>,
    pub breakpoints_used: usize,
    pub has_uncached_runtime_context: bool,
}

// Stable/dynamic cut shared with the OpenAI wire and the core stable hash;
// the header list and match semantics live in `crate::providers::shared`.
const RUNTIME_CONTEXT_SECTION_HEADER: &str = "[Runtime Context]";
const HISTORY_DIRECTIVES_SECTION_HEADER: &str = "[History Directives]";
const RUNTIME_CONTEXT_NEWLINE: &str = concat!("[Runtime Context]", "\n");
const NEWLINE_RUNTIME_CONTEXT_NEWLINE: &str = concat!("\n", "[Runtime Context]", "\n");
const NEWLINE_HISTORY_DIRECTIVES_NEWLINE: &str = concat!("\n", "[History Directives]", "\n");
const HISTORY_DIRECTIVES_NEWLINE: &str = concat!("[History Directives]", "\n");

fn has_runtime_context_section(prompt: &str) -> bool {
    prompt.starts_with(RUNTIME_CONTEXT_NEWLINE)
        || prompt.contains(NEWLINE_RUNTIME_CONTEXT_NEWLINE)
        || prompt.starts_with(HISTORY_DIRECTIVES_NEWLINE)
        || prompt.contains(NEWLINE_HISTORY_DIRECTIVES_NEWLINE)
}

fn append_history_system_directives(
    final_system_prompt: &mut String,
    request: &LLMRequest,
    include_turn_scoped_system_directives: bool,
) {
    let directives: Vec<String> = request
        .messages
        .iter()
        .filter(|message| message.role == MessageRole::System)
        .filter(|message| include_turn_scoped_system_directives || message.clear_at.is_none())
        .map(|message| message.content.as_text().trim().to_string())
        .filter(|text| !text.is_empty())
        .collect();

    if directives.is_empty() {
        return;
    }

    if !has_runtime_context_section(final_system_prompt) {
        if !final_system_prompt.is_empty() && !final_system_prompt.ends_with('\n') {
            final_system_prompt.push('\n');
        }
        final_system_prompt.push_str(RUNTIME_CONTEXT_SECTION_HEADER);
        final_system_prompt.push('\n');
    } else if !final_system_prompt.ends_with('\n') {
        final_system_prompt.push('\n');
    }

    final_system_prompt.push_str(HISTORY_DIRECTIVES_SECTION_HEADER);
    final_system_prompt.push('\n');
    for directive in directives {
        final_system_prompt.push_str("- ");
        final_system_prompt.push_str(&directive);
        final_system_prompt.push('\n');
    }
}

fn split_runtime_context_section(prompt: &str) -> Option<(String, String)> {
    // Cut at the earliest dynamic header so earlier runtime sections
    // (planning, harness limits, tool catalog, environment) stay out of the
    // cached prefix.
    let (stable_prefix, dynamic) = split_dynamic_prompt_suffix(prompt);
    let runtime_section = dynamic.filter(|section| !section.is_empty())?;
    if stable_prefix.is_empty() && runtime_section.is_empty() {
        return None;
    }
    Some((stable_prefix, runtime_section))
}

pub(crate) fn build_system_prompt(
    request: &LLMRequest,
    cache_control: &Option<CacheControl>,
    breakpoints_remaining: usize,
    include_turn_scoped_system_directives: bool,
) -> SystemPromptBuildResult {
    let mut final_system_prompt = request
        .system_prompt
        .as_ref()
        .map(|s| s.as_ref())
        .unwrap_or_default()
        .to_string();

    append_history_system_directives(&mut final_system_prompt, request, include_turn_scoped_system_directives);

    if final_system_prompt.is_empty() {
        return SystemPromptBuildResult {
            system_value: None,
            breakpoints_used: 0,
            has_uncached_runtime_context: false,
        };
    }

    if let Some((stable_prefix, runtime_section)) = split_runtime_context_section(&final_system_prompt) {
        let should_cache_stable_prefix =
            cache_control.is_some() && breakpoints_remaining > 0 && !stable_prefix.is_empty();
        let mut blocks = Vec::new();

        if !stable_prefix.is_empty() {
            if should_cache_stable_prefix {
                if let Some(cc) = cache_control.as_ref() {
                    blocks.push(json!({
                        "type": "text",
                        "text": stable_prefix,
                        "cache_control": cc
                    }));
                }
            } else {
                blocks.push(json!({
                    "type": "text",
                    "text": stable_prefix
                }));
            }
        }

        blocks.push(json!({
            "type": "text",
            "text": runtime_section
        }));

        return SystemPromptBuildResult {
            system_value: Some(Value::Array(blocks)),
            breakpoints_used: usize::from(should_cache_stable_prefix),
            has_uncached_runtime_context: true,
        };
    }

    let should_cache = cache_control.is_some() && breakpoints_remaining > 0;

    if should_cache && let Some(cc) = cache_control.as_ref() {
        let block = json!({
            "type": "text",
            "text": final_system_prompt.trim(),
            "cache_control": cc
        });
        return SystemPromptBuildResult {
            system_value: Some(Value::Array(vec![block])),
            breakpoints_used: 1,
            has_uncached_runtime_context: false,
        };
    }

    SystemPromptBuildResult {
        system_value: Some(Value::String(final_system_prompt.trim().to_string())),
        breakpoints_used: 0,
        has_uncached_runtime_context: false,
    }
}
