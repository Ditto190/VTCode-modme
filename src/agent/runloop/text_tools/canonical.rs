use serde_json::Value;
use vtcode_core::config::constants::tools;

pub(super) const TEXTUAL_TOOL_PREFIXES: &[&str] = &["default_api."];
pub(super) const DIRECT_FUNCTION_ALIASES: &[&str] = &[
    "run",
    "run_cmd",
    "runcommand",
    "terminalrun",
    "terminal_cmd",
    "terminalcommand",
];

/// Maximum accepted tool-name length for content-derived calls.
pub(crate) const MAX_CLEAN_TOOL_NAME_LEN: usize = 64;

/// Whether `raw` is a clean tool identifier (not prose, not a fixture blob).
///
/// Content-derived tool-call extraction must only bind names that look like
/// real tool identifiers. Mid-prose mentions of `tool_call` produce fragments
/// (backticks, spaces, em-dashes); rejecting those keeps documentation from
/// becoming a dispatchable call.
pub(crate) fn is_clean_tool_name(raw: &str) -> bool {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_CLEAN_TOOL_NAME_LEN {
        return false;
    }
    let mut chars = trimmed.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() {
        return false;
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

/// Whether `raw` is safe to dispatch as a native tool name (not a prose blob).
///
/// Allows MCP-visible names (`mcp__github__list`) and names with `:`/`-`/`.`
/// while rejecting whitespace/newlines/backticks and absurd lengths that mark
/// model analysis text accidentally placed in the name field.
pub(crate) fn is_dispatchable_tool_name(raw: &str) -> bool {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_CLEAN_TOOL_NAME_LEN {
        return false;
    }
    if trimmed
        .chars()
        .any(|ch| ch.is_whitespace() || matches!(ch, '`' | '\u{2014}' | '\u{2013}'))
    {
        return false;
    }
    true
}

/// Map well-known shell aliases used by gateway models onto `exec_command`.
///
/// Returns `None` when the name is not a shell alias (leave it unchanged).
/// Also used for native tool calls so `bash`/`shell`/`run` do not fail as
/// `Unknown tool` after the textual path already understood them.
pub(crate) fn canonicalize_shell_tool_alias(raw: &str) -> Option<String> {
    let normalized = canonicalize_normalized_name(raw)?;
    // Only true shell aliases. PTY-family names (run_pty_cmd, create_pty_session)
    // keep their own tool identity and must not collapse into exec_command.
    matches!(
        normalized.as_str(),
        "run"
            | "runcmd"
            | "runcommand"
            | "terminalrun"
            | "terminalcmd"
            | "terminalcommand"
            | "command"
            | "shell"
            | "bash"
            | "container_exec"
            | "exec"
            | "exec_command"
    )
    .then(|| tools::EXEC_COMMAND.to_string())
}

#[derive(Clone, Copy)]
pub(super) struct ExecCommandDefaults {
    pub(super) action: &'static str,
    pub(super) force_tty: Option<bool>,
}

pub(super) fn canonicalize_tool_name(raw: &str) -> Option<String> {
    let normalized = canonicalize_normalized_name(raw)?;
    if normalized.is_empty() {
        None
    } else if exec_command_defaults_from_normalized_name(&normalized).is_some() {
        Some(tools::EXEC_COMMAND.to_string())
    } else {
        Some(normalized)
    }
}

/// Canonicalize a tool call and optionally validate against known tools.
///
/// When `validate` is true, the result is checked against the known-tool allowlist.
/// When `validate` is false, only canonicalisation and exec_command defaults are applied.
pub(super) fn canonicalize_tool_result(name: String, mut args: Value, validate: bool) -> Option<(String, Value)> {
    let normalized = canonicalize_normalized_name(&name)?;
    let canonical = canonicalize_tool_name(&name)?;
    if canonical == tools::EXEC_COMMAND
        && let Some(defaults) = exec_command_defaults_from_normalized_name(&normalized)
        && let Some(payload) = args.as_object_mut()
    {
        apply_exec_command_defaults(payload, defaults);
    }
    if validate {
        if is_known_textual_tool(&canonical) {
            Some((canonical, args))
        } else {
            None
        }
    } else {
        Some((canonical, args))
    }
}

pub(crate) fn is_known_textual_tool(name: &str) -> bool {
    matches!(
        name,
        tools::WRITE_FILE
            | tools::EDIT_FILE
            | tools::READ_FILE
            | tools::EXEC_COMMAND
            | tools::WRITE_STDIN
            | tools::CODE_SEARCH
            | tools::GREP_FILE
            | tools::LIST_FILES
            | tools::APPLY_PATCH
            | tools::RESIZE_PTY_SESSION
            | tools::CREATE_FILE
            | tools::DELETE_FILE
            | tools::SEARCH_REPLACE
            | tools::UNIFIED_FILE
            | tools::TASK_TRACKER
    )
}

fn canonicalize_normalized_name(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let trimmed = trimmed.trim_matches(|ch| matches!(ch, '"' | '\'' | '`'));

    let mut normalized = String::with_capacity(trimmed.len());
    let mut last_was_separator = false;
    for ch in trimmed.chars() {
        if ch.is_ascii_alphanumeric() {
            normalized.push(ch.to_ascii_lowercase());
            last_was_separator = false;
        } else if ch == '_' {
            normalized.push('_');
            last_was_separator = false;
        } else if matches!(ch, ' ' | '\t' | '\n' | '-' | ':' | '.') && !last_was_separator && !normalized.is_empty() {
            normalized.push('_');
            last_was_separator = true;
        }
    }

    let normalized = normalized.trim_matches('_').to_string();
    if normalized.is_empty() { None } else { Some(normalized) }
}

pub(super) fn exec_command_defaults_for_name(raw: &str) -> Option<ExecCommandDefaults> {
    canonicalize_normalized_name(raw)
        .and_then(|normalized| exec_command_defaults_from_normalized_name(normalized.as_str()))
}

pub(super) fn apply_exec_command_defaults(payload: &mut serde_json::Map<String, Value>, defaults: ExecCommandDefaults) {
    payload
        .entry("action".to_string())
        .or_insert_with(|| Value::String(defaults.action.to_string()));
    if let Some(tty) = defaults.force_tty {
        payload.entry("tty".to_string()).or_insert_with(|| Value::Bool(tty));
    }
}

fn exec_command_defaults_from_normalized_name(normalized: &str) -> Option<ExecCommandDefaults> {
    match normalized {
        "run" | "runcmd" | "runcommand" | "terminalrun" | "terminalcmd" | "terminalcommand" | "command" | "shell"
        | "bash" | "container_exec" | "exec" | "exec_command" => {
            Some(ExecCommandDefaults { action: "run", force_tty: None })
        }
        "run_pty_cmd" | "create_pty_session" => Some(ExecCommandDefaults { action: "run", force_tty: Some(true) }),
        "send_pty_input" => Some(ExecCommandDefaults { action: "write", force_tty: None }),
        "read_pty_session" => Some(ExecCommandDefaults { action: "poll", force_tty: None }),
        "list_pty_sessions" => Some(ExecCommandDefaults { action: "list", force_tty: None }),
        "close_pty_session" => Some(ExecCommandDefaults { action: "close", force_tty: None }),
        _ => None,
    }
}
