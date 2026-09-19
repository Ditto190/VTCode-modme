use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use serde_json::Value;
use tokio::sync::Notify;
use vtcode_commons::modal_hints::{APPROVAL_NAVIGATE_CANCEL, APPROVAL_NAVIGATE_DENY, choose_handling_line};
use vtcode_core::core::interfaces::ui::UiSession;
use vtcode_core::notifications::{NotificationEvent, send_global_notification};
use vtcode_core::sandboxing::{AdditionalPermissions, SandboxPermissions as CoreSandboxPermissions};
use vtcode_core::utils::ansi::AnsiRenderer;
use vtcode_ui::tui::app::{
    InlineHandle, ListOverlayRequest, TransientHotkey, TransientHotkeyAction, TransientHotkeyKey, TransientRequest,
    TransientSubmission,
};

use super::HitlDecision;
use super::shell_approval::{ApprovalLearningTarget, PersistentApprovalTarget};
use crate::agent::runloop::tool_output::format_unified_diff_lines;
use crate::agent::runloop::unified::overlay_prompt::{OverlayWaitOutcome, show_overlay_and_wait};
use crate::agent::runloop::unified::state::CtrlCState;
use crate::agent::runloop::unified::ui_interaction::PlaceholderGuard;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ToolPermissionPromptKind {
    Standard,
    Mcp,
}

#[cold]
fn cancelled_prompt_decision(prompt_kind: ToolPermissionPromptKind) -> HitlDecision {
    if prompt_kind == ToolPermissionPromptKind::Mcp {
        HitlDecision::DeniedOnce
    } else {
        HitlDecision::Denied
    }
}

fn shell_run_args<'a>(tool_name: &str, tool_args: Option<&'a Value>) -> Option<&'a Value> {
    let args = tool_args?;
    vtcode_core::tools::tool_intent::is_command_run_tool_call(tool_name, args).then_some(args)
}

pub(super) fn tool_permission_prompt_kind(tool_name: &str) -> ToolPermissionPromptKind {
    if vtcode_core::tools::mcp::is_legacy_mcp_tool_name(tool_name)
        || vtcode_core::tools::mcp::parse_canonical_mcp_tool_name(tool_name).is_some()
        || tool_name.starts_with(vtcode_core::tools::mcp::MCP_QUALIFIED_TOOL_PREFIX)
    {
        ToolPermissionPromptKind::Mcp
    } else {
        ToolPermissionPromptKind::Standard
    }
}

fn normalized_shell_command_value(args: &Value) -> Option<Value> {
    match vtcode_core::tools::command_args::normalize_shell_args(args) {
        Ok(normalized) => vtcode_core::tools::command_args::normalized_command_value(&normalized)
            .ok()
            .flatten(),
        Err(_) => vtcode_core::tools::command_args::normalized_command_value(args).ok().flatten(),
    }
}

fn extract_shell_command_text_from_run_args(args: &Value) -> Option<String> {
    vtcode_core::tools::command_args::command_text(args).ok().flatten()
}

fn extract_shell_command_words_from_run_args(args: &Value) -> Option<Vec<String>> {
    vtcode_core::tools::command_args::command_words(args).ok().flatten()
}

fn render_shell_command_words(parts: &[String]) -> String {
    shell_words::join(parts.iter().map(|part| part.as_str()))
}

fn shell_command_contains_control_operators(command: &str) -> bool {
    command.contains("&&")
        || command.contains("||")
        || command.contains('|')
        || command.contains(';')
        || command.contains("$(")
        || command.contains('`')
        || command.contains("<<")
        || command.contains('\n')
}

fn shell_run_uses_nested_shell(command_words: &[String]) -> bool {
    let Some(program) = command_words.first() else {
        return false;
    };
    let basename = Path::new(program)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(program)
        .to_ascii_lowercase();
    let args = &command_words[1..];

    match basename.as_str() {
        "sh" | "bash" | "zsh" | "fish" => args
            .iter()
            .any(|arg| matches!(arg.as_str(), "-c" | "-ic" | "-lc" | "--command" | "--login" | "--interactive")),
        "pwsh" | "powershell" | "powershell.exe" => args
            .iter()
            .any(|arg| arg.eq_ignore_ascii_case("-command") || arg.eq_ignore_ascii_case("-c")),
        "cmd" | "cmd.exe" => args
            .iter()
            .any(|arg| arg.eq_ignore_ascii_case("/c") || arg.eq_ignore_ascii_case("/k")),
        _ => false,
    }
}

fn shell_command_supports_persistent_approval(args: &Value, command_words: &[String]) -> bool {
    let Some(command_value) = normalized_shell_command_value(args) else {
        return false;
    };

    match command_value {
        Value::String(command) => {
            let trimmed = command.trim();
            !trimmed.is_empty()
                && !shell_command_contains_control_operators(trimmed)
                && !shell_run_uses_nested_shell(command_words)
        }
        Value::Array(_) => !shell_run_uses_nested_shell(command_words),
        _ => false,
    }
}

fn parse_shell_sandbox_permissions(args: &Value) -> CoreSandboxPermissions {
    let requested_permissions = args
        .get("sandbox_permissions")
        .cloned()
        .map(serde_json::from_value::<CoreSandboxPermissions>)
        .transpose()
        .ok()
        .flatten()
        .unwrap_or(CoreSandboxPermissions::UseDefault);

    requested_permissions.normalized_for(parse_shell_additional_permissions(args).as_ref())
}

fn parse_shell_additional_permissions(args: &Value) -> Option<AdditionalPermissions> {
    args.get("additional_permissions")
        .cloned()
        .and_then(|value| serde_json::from_value::<AdditionalPermissions>(value).ok())
        .filter(|permissions| !permissions.is_empty())
}

fn shell_additional_permissions_present(args: &Value) -> bool {
    parse_shell_additional_permissions(args).is_some()
}

fn shell_permission_scope_suffix(args: &Value) -> String {
    let sandbox_permissions = parse_shell_sandbox_permissions(args);
    let sandbox_permissions =
        serde_json::to_string(&sandbox_permissions).unwrap_or_else(|_| "\"use_default\"".to_string());
    let additional_permissions = args
        .get("additional_permissions")
        .filter(|_| shell_additional_permissions_present(args))
        .map(|value| serde_json::to_string(value).unwrap_or_else(|_| "\"<invalid_additional>\"".to_string()))
        .unwrap_or_else(|| "null".to_string());

    format!("sandbox_permissions={sandbox_permissions}|additional_permissions={additional_permissions}")
}

fn extract_shell_approval_justification(tool_name: &str, tool_args: Option<&Value>) -> Option<String> {
    let args = shell_run_args(tool_name, tool_args)?;
    args.get("justification")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub(super) fn extract_shell_command_text(tool_name: &str, tool_args: Option<&Value>) -> Option<String> {
    let args = shell_run_args(tool_name, tool_args)?;
    extract_shell_command_text_from_run_args(args)
}

pub(super) fn extract_shell_raw_command_text(tool_name: &str, tool_args: Option<&Value>) -> Option<String> {
    let args = shell_run_args(tool_name, tool_args)?;
    vtcode_core::tools::command_args::raw_command_text(args)
}

pub(super) fn split_command_words_on_operators(parts: &[String]) -> Option<Vec<Vec<String>>> {
    if parts.is_empty() {
        return None;
    }

    let mut segments: Vec<Vec<String>> = Vec::new();
    let mut current: Vec<String> = Vec::new();

    for word in parts {
        match word.as_str() {
            "|" | "||" | "&&" | ";" => {
                if !current.is_empty() {
                    segments.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(word.clone()),
        }
    }

    if !current.is_empty() {
        segments.push(current);
    }

    if segments.is_empty() || segments.iter().all(|s| s.is_empty()) {
        return None;
    }

    Some(segments)
}

pub(super) fn extract_shell_approval_command_words(tool_name: &str, tool_args: Option<&Value>) -> Option<Vec<String>> {
    let args = shell_run_args(tool_name, tool_args)?;
    extract_shell_command_words_from_run_args(args)
}

pub(super) fn extract_shell_approval_command_prefix_words(
    tool_name: &str,
    tool_args: Option<&Value>,
) -> Option<Vec<String>> {
    let args = shell_run_args(tool_name, tool_args)?;
    let command_words = extract_shell_approval_command_words(tool_name, tool_args)?;
    shell_command_supports_persistent_approval(args, &command_words).then_some(command_words)
}

pub(super) fn extract_shell_permission_scope_signature(tool_name: &str, tool_args: Option<&Value>) -> Option<String> {
    let args = shell_run_args(tool_name, tool_args)?;
    Some(shell_permission_scope_suffix(args))
}

pub(super) fn extract_shell_approval_scope_signature(tool_name: &str, tool_args: Option<&Value>) -> Option<String> {
    let args = shell_run_args(tool_name, tool_args)?;
    let command_words = extract_shell_approval_command_words(tool_name, tool_args)?;
    shell_command_supports_persistent_approval(args, &command_words).then(|| shell_permission_scope_suffix(args))
}

pub(super) fn extract_shell_persistent_approval_prefix_rule(
    tool_name: &str,
    tool_args: Option<&Value>,
) -> Option<Vec<String>> {
    let args = shell_run_args(tool_name, tool_args)?;
    let command_words = extract_shell_command_words_from_run_args(args)?;
    if !shell_command_supports_persistent_approval(args, &command_words) {
        return None;
    }

    let prefix_rule = args
        .get("prefix_rule")
        .and_then(Value::as_array)?
        .iter()
        .map(|value| value.as_str().map(ToOwned::to_owned))
        .collect::<Option<Vec<_>>>()?;

    if prefix_rule.is_empty() || prefix_rule.len() > command_words.len() {
        return None;
    }

    prefix_rule
        .iter()
        .zip(command_words.iter())
        .all(|(prefix, command)| prefix == command)
        .then_some(prefix_rule)
}

pub(super) fn render_shell_persistent_approval_prefix_entry(
    tool_name: &str,
    tool_args: Option<&Value>,
    prefix_rule: &[String],
) -> Option<String> {
    let scope_signature = extract_shell_approval_scope_signature(tool_name, tool_args)?;
    Some(format!("{}|{}", render_shell_command_words(prefix_rule), scope_signature))
}

pub(super) fn render_shell_approval_command_words(parts: &[String]) -> String {
    render_shell_command_words(parts)
}

pub(super) fn shell_permission_cache_suffix(tool_name: &str, tool_args: Option<&Value>) -> Option<String> {
    let args = shell_run_args(tool_name, tool_args)?;
    let command = extract_shell_command_text_from_run_args(args)?;
    let scope_suffix = shell_permission_scope_suffix(args);

    if parse_shell_sandbox_permissions(args) == CoreSandboxPermissions::UseDefault
        && !shell_additional_permissions_present(args)
    {
        return Some(command);
    }

    Some(format!("{command}|{scope_suffix}"))
}

#[cfg(test)]
pub(super) fn shell_allows_persistent_decisions(tool_name: &str, tool_args: Option<&Value>) -> bool {
    extract_shell_command_text(tool_name, tool_args).is_none()
}

#[cfg(test)]
fn truncate_arg_preview(value: &str) -> String {
    const MAX_CHARS: usize = 60;
    const TRUNCATED_CHARS: usize = 57;
    if value.chars().nth(MAX_CHARS).is_some() {
        let mut truncated: String = value.chars().take(TRUNCATED_CHARS).collect();
        truncated.push_str("...");
        truncated
    } else {
        value.to_string()
    }
}

fn shell_command_preview_lines(tool_name: &str, tool_args: Option<&Value>) -> Option<Vec<String>> {
    let command = extract_shell_command_text(tool_name, tool_args)?;
    let command = command.trim();
    (!command.is_empty()).then(|| command.lines().map(str::to_string).collect())
}

/// Full, untruncated URL for web-fetch approval modals. The modal description
/// must show the exact fetch target (not just the domain) so approval is
/// informed. No truncation is applied here; modal wrapping handles width.
fn web_fetch_approval_url(tool_name: &str, tool_args: Option<&Value>) -> Option<String> {
    let canonical = vtcode_core::tools::names::canonical_tool_name(tool_name);
    let is_web_fetch = canonical == vtcode_core::config::constants::tools::WEB_FETCH
        || canonical == vtcode_core::config::constants::tools::FETCH_URL
        || canonical == vtcode_core::config::constants::tools::FETCH
        || canonical == vtcode_core::config::constants::tools::DEFUDDLE_FETCH
        || tool_name == vtcode_core::config::constants::tools::FETCH;
    if !is_web_fetch {
        return None;
    }
    tool_args?
        .as_object()?
        .get("url")?
        .as_str()
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .map(ToOwned::to_owned)
}

/// Max logical rows shown for the approved command block; longer commands
/// collapse behind an overflow count so the popup fits its viewport budget.
const MAX_COMMAND_PREVIEW_LINES: usize = 8;
/// Max chars per command line. Middle truncation preserves both the executable
/// prefix and trailing flags/destinations that can materially change behavior.
const MAX_COMMAND_LINE_CHARS: usize = 120;
/// Max chars for persistent-approval display labels embedded in option
/// subtitles; the dedicated command block carries the reviewable invocation.
const MAX_APPROVAL_LABEL_CHARS: usize = 60;
/// Max chars per context row (modal wrapping handles visual width).
const MAX_CONTEXT_LINE_CHARS: usize = 160;
const MAX_RISK_LINE_CHARS: usize = 32;

fn format_command_preview_line(line: &str) -> String {
    format!("`{}`", vtcode_commons::formatting::truncate_middle(line, MAX_COMMAND_LINE_CHARS))
}

/// Format command lines as bounded head + tail evidence. Keeping the tail is
/// required for informed approval because destructive flags and destinations
/// commonly occur at the end of an invocation.
fn format_command_preview_lines(command_lines: Vec<String>) -> Vec<String> {
    if command_lines.len() <= MAX_COMMAND_PREVIEW_LINES {
        return command_lines.iter().map(|line| format_command_preview_line(line)).collect();
    }
    const HEAD_LINES: usize = 5;
    const TAIL_LINES: usize = 2;
    let hidden = command_lines.len().saturating_sub(HEAD_LINES + TAIL_LINES);
    let mut preview = Vec::with_capacity(MAX_COMMAND_PREVIEW_LINES);
    preview.extend(
        command_lines
            .iter()
            .take(HEAD_LINES)
            .map(|line| format_command_preview_line(line)),
    );
    preview.push(format!("… +{hidden} more lines (full command runs on approval)"));
    preview.extend(
        command_lines
            .iter()
            .skip(command_lines.len().saturating_sub(TAIL_LINES))
            .map(|line| format_command_preview_line(line)),
    );
    preview
}

/// Friendly label for the agent's goal in the permission popup. The modal
/// renderer styles this as a subordinate context row (no bullet), matching
/// the previous `Reason:` treatment without the technical `WHY` header.
const AGENT_GOAL_LABEL: &str = "What the agent is trying to do";

fn push_context_line(description_lines: &mut Vec<String>, label: &str, value: &str) -> bool {
    push_capped_context_line(description_lines, label, value, MAX_CONTEXT_LINE_CHARS)
}

fn push_capped_context_line(description_lines: &mut Vec<String>, label: &str, value: &str, max_chars: usize) -> bool {
    let first = value.lines().map(str::trim).find(|line| !line.is_empty()).unwrap_or("");
    if !first.is_empty() {
        description_lines.push(format!("{label}: {}", vtcode_commons::formatting::truncate_middle(first, max_chars)));
        true
    } else {
        false
    }
}

/// Compact agent-justification lines for the approval dialog: agent goal +
/// `Risk` only so the popup stays minimal and scannable. Full reasoning
/// (including expected outcome) remains in logs.
fn compact_justification_lines(just: &vtcode_core::tools::ToolJustification) -> Vec<String> {
    let mut lines = Vec::new();
    let reason = just.reason.lines().map(str::trim).find(|line| !line.is_empty()).unwrap_or("");
    if !reason.is_empty() {
        lines.push(format!(
            "{AGENT_GOAL_LABEL}: {}",
            vtcode_commons::formatting::truncate_middle(reason, MAX_CONTEXT_LINE_CHARS)
        ));
    }
    let risk = just.risk_level.trim();
    if !risk.is_empty() {
        lines.push(format!("Risk: {}", vtcode_commons::formatting::truncate_middle(risk, MAX_RISK_LINE_CHARS)));
    }
    lines
}

fn tool_args_diff_preview(tool_name: &str, tool_args: Option<&Value>) -> Option<Vec<String>> {
    let args = tool_args?.as_object()?;
    let (before, after) = match tool_name {
        "edit_file" => {
            let old_str = args.get("old_str").or_else(|| args.get("old_string")).and_then(Value::as_str)?;
            let new_str = args.get("new_str").or_else(|| args.get("new_string")).and_then(Value::as_str)?;
            (Some(old_str), new_str)
        }
        "write_file" | "create_file" => {
            let content = args.get("content").and_then(Value::as_str)?;
            (None, content)
        }
        "file_operation" => {
            let action = args
                .get("action")
                .and_then(Value::as_str)
                .or_else(|| {
                    if args.get("old_str").is_some() || args.get("old_string").is_some() {
                        Some("edit")
                    } else if args.get("content").is_some() {
                        Some("write")
                    } else {
                        None
                    }
                })
                .unwrap_or("read");

            match action {
                "edit" => {
                    let old_str = args.get("old_str").or_else(|| args.get("old_string")).and_then(Value::as_str)?;
                    let new_str = args.get("new_str").or_else(|| args.get("new_string")).and_then(Value::as_str)?;
                    (Some(old_str), new_str)
                }
                "write" | "create" => {
                    let content = args.get("content").and_then(Value::as_str)?;
                    (None, content)
                }
                _ => return None,
            }
        }
        _ => return None,
    };
    let path = args
        .get("path")
        .or_else(|| args.get("file_path"))
        .or_else(|| args.get("target_path"))
        .and_then(Value::as_str)
        .unwrap_or("(unknown file)");

    let diff_preview = vtcode_core::tools::file_ops::build_diff_preview(path, before, after);
    let skipped = diff_preview.get("skipped").and_then(Value::as_bool).unwrap_or(false);
    if skipped {
        let reason = diff_preview
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or("preview unavailable");
        return Some(vec![format!("diff: {}", reason)]);
    }

    let content = diff_preview.get("content").and_then(Value::as_str).unwrap_or("");
    if content.is_empty() {
        return Some(vec!["(no changes)".to_string()]);
    }

    let plain = vtcode_commons::ansi::strip_ansi(content);
    let lines = format_unified_diff_lines(&plain);
    if lines.len() <= 80 {
        return Some(lines);
    }

    let mut preview = Vec::with_capacity(61);
    preview.extend(lines.iter().take(40).cloned());
    preview.push(format!("… +{} lines", lines.len().saturating_sub(60)));
    preview.extend(lines.iter().skip(lines.len().saturating_sub(20)).cloned());
    Some(preview)
}

/// Intro rows for the approval popup, written as plain permission language.
/// Shell commands get a dedicated command block below, so their truncated
/// action summary is omitted. Other tools reuse the human-friendly action
/// summary directly in the intro sentence.
fn tool_permission_header_lines(
    tool_name: &str,
    display_name: &str,
    has_command_preview: bool,
    source_thread_label: Option<&str>,
) -> Vec<String> {
    let mut lines = Vec::new();
    if has_command_preview {
        lines.push("The agent wants to run a shell command and needs your approval.".to_string());
    } else if !display_name.trim().is_empty() {
        let trimmed = display_name.trim();
        // `describe_tool_action` prefixes MCP tools with "MCP " (e.g. "MCP Search
        // code for docs"); move it to a trailing "via MCP" so the sentence keeps
        // verb agreement and the acronym is never lowercased mid-word.
        if let Some(rest) = trimmed.strip_prefix("MCP ") {
            lines.push(format!("The agent wants to {} via MCP and needs your approval.", lowercase_first_action(rest)));
        } else {
            lines.push(format!("The agent wants to {} and needs your approval.", lowercase_first_action(trimmed)));
        }
    } else {
        lines.push(format!("The agent wants to use {tool_name} and needs your approval."));
    }
    if let Some(source_label) = source_thread_label {
        lines.push(format!("Requested from: {source_label}"));
    }
    lines
}

/// Lowercase the initial of a Titlecase action ("Edit file" → "edit file") so
/// it fits mid-sentence. Leading acronyms ("MCP", "URL") are left alone so
/// they are never mangled into "mCP"/"uRL".
fn lowercase_first_action(action: &str) -> String {
    let mut chars = action.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => {
            let rest = chars.as_str();
            let next_is_upper = rest.chars().next().is_some_and(|next| next.is_uppercase());
            if first.is_uppercase() && !next_is_upper {
                first.to_lowercase().collect::<String>() + rest
            } else {
                action.to_string()
            }
        }
    }
}

/// Context rows explaining what the agent is trying to do and the risk level.
/// Pure helper so the dedup/fallback rules stay unit-testable without a UI
/// session: at most one agent-goal row (an explicit reason wins over the
/// ledger justification), plus `Risk` when present, plus a fallback sentence
/// when the agent provided no details at all.
fn build_permission_context_lines(
    approval_reason: Option<&str>,
    shell_justification: Option<&str>,
    justification: Option<&vtcode_core::tools::ToolJustification>,
) -> Vec<String> {
    let mut lines = Vec::new();
    let mut goal_emitted = false;

    if let Some(reason) = approval_reason {
        goal_emitted = push_context_line(&mut lines, AGENT_GOAL_LABEL, reason);
    } else if let Some(shell_reason) = shell_justification {
        goal_emitted = push_context_line(&mut lines, AGENT_GOAL_LABEL, shell_reason);
    }

    if let Some(just) = justification {
        // `compact_justification_lines` emits the agent goal + Risk only; skip
        // a duplicate goal row when an explicit reason already covers it.
        for line in compact_justification_lines(just) {
            if line.starts_with(AGENT_GOAL_LABEL) && goal_emitted {
                continue;
            }
            lines.push(line);
        }
    }

    if lines.is_empty() {
        lines.push("No additional details were provided. Review the action above before allowing.".to_string());
    }

    lines
}

fn build_tool_permission_options(
    prompt_kind: ToolPermissionPromptKind,
    persistent_approval_target: Option<&PersistentApprovalTarget>,
) -> Vec<vtcode_ui::tui::app::InlineListItem> {
    use vtcode_ui::tui::app::{InlineListItem, InlineListSelection};

    let mut options = vec![
        InlineListItem {
            title: "Approve Once".to_string(),
            subtitle: Some("Allow this time only".to_string()),
            badge: None,
            indent: 0,
            selection: Some(InlineListSelection::ToolApproval(true)),
            search_value: Some("approve yes allow once y 1".to_string()),
        },
        InlineListItem {
            title: if prompt_kind == ToolPermissionPromptKind::Mcp {
                "Approve this session".to_string()
            } else {
                "Allow for Session".to_string()
            },
            subtitle: if prompt_kind == ToolPermissionPromptKind::Mcp {
                Some("Remember for this session".to_string())
            } else {
                Some("For the current session".to_string())
            },
            badge: Some("Session".to_string()),
            indent: 0,
            selection: Some(InlineListSelection::ToolApprovalSession),
            search_value: Some("session temporary temp 2".to_string()),
        },
    ];

    if let Some(target) = persistent_approval_target {
        let short_label = match target {
            PersistentApprovalTarget::ToolLevel => "this tool".to_string(),
            PersistentApprovalTarget::ExactInvocation { display_label }
            | PersistentApprovalTarget::PrefixRule { display_label, .. } => {
                vtcode_commons::modal_hints::truncate_modal_text(display_label, MAX_APPROVAL_LABEL_CHARS)
            }
        };
        let subtitle = format!("Remember {short_label} in this workspace");
        options.push(InlineListItem {
            title: "Always approve and save to policy cache".to_string(),
            subtitle: Some(subtitle),
            badge: Some("Permanent".to_string()),
            indent: 0,
            selection: Some(InlineListSelection::ToolApprovalPermanent),
            search_value: Some("always permanent forever save 3".to_string()),
        });
    }

    options.push(InlineListItem {
        title: "".to_string(),
        subtitle: None,
        badge: None,
        indent: 0,
        selection: None,
        search_value: None,
    });

    options.push(InlineListItem {
        title: if prompt_kind == ToolPermissionPromptKind::Mcp {
            "Cancel".to_string()
        } else {
            "Deny Once".to_string()
        },
        subtitle: if prompt_kind == ToolPermissionPromptKind::Mcp {
            Some("Cancel this tool call".to_string())
        } else {
            Some("Ask again next time".to_string())
        },
        badge: None,
        indent: 0,
        selection: Some(InlineListSelection::ToolApprovalDenyOnce),
        search_value: Some(if prompt_kind == ToolPermissionPromptKind::Mcp {
            "cancel stop reject decline 4".to_string()
        } else {
            "deny no reject once temporary 4".to_string()
        }),
    });

    if matches!(persistent_approval_target, Some(PersistentApprovalTarget::ToolLevel))
        && prompt_kind != ToolPermissionPromptKind::Mcp
    {
        options.push(InlineListItem {
            title: "Always Deny".to_string(),
            subtitle: Some("Block this tool until policy is changed".to_string()),
            badge: Some("Persistent".to_string()),
            indent: 0,
            selection: Some(InlineListSelection::ToolApproval(false)),
            search_value: Some("deny no reject cancel never always 5".to_string()),
        });
    }

    options
}

pub(super) async fn prompt_tool_permission<S: UiSession + ?Sized>(
    display_name: &str,
    tool_name: &str,
    tool_args: Option<&Value>,
    learning_target: &ApprovalLearningTarget,
    renderer: &mut AnsiRenderer,
    handle: &InlineHandle,
    session: &mut S,
    ctrl_c_state: &Arc<CtrlCState>,
    ctrl_c_notify: &Arc<Notify>,
    default_placeholder: Option<String>,
    approval_reason: Option<&str>,
    justification: Option<&vtcode_core::tools::ToolJustification>,
    persistent_approval_target: Option<&PersistentApprovalTarget>,
    approval_recorder: Option<&vtcode_core::tools::ApprovalRecorder>,
    hitl_notification_bell: bool,
    source_thread_label: Option<&str>,
) -> Result<HitlDecision> {
    // Auto-approve if the approval recorder indicates high-frequency approval
    // history (≥3 approvals with >80% approval rate) for the exact invocation
    // OR any broader learned pattern (e.g. safe `find <subdir>` family).
    if let Some(recorder) = approval_recorder {
        for (key, _label) in learning_target.iter_keys() {
            if recorder.should_auto_approve(key).await {
                tracing::info!(
                    tool = %tool_name,
                    key = %key,
                    "Auto-approved based on learned approval pattern"
                );
                return Ok(HitlDecision::Approved);
            }
        }
    }
    let prompt_kind = tool_permission_prompt_kind(tool_name);
    let command_preview = shell_command_preview_lines(tool_name, tool_args);
    let diff_preview = command_preview
        .is_none()
        .then(|| tool_args_diff_preview(tool_name, tool_args))
        .flatten();
    let mut description_lines =
        tool_permission_header_lines(tool_name, display_name, command_preview.is_some(), source_thread_label);

    if let Some(url) = web_fetch_approval_url(tool_name, tool_args) {
        let url_line = format!("URL: {url}");
        if !description_lines.iter().any(|line| line.contains(&url)) {
            description_lines.push(url_line);
        }
    }

    if let Some(command_lines) = command_preview {
        description_lines.extend(format_command_preview_lines(command_lines));
    }

    if let Some(diff_lines) = diff_preview {
        description_lines.extend(diff_lines);
    }

    let shell_justification = extract_shell_approval_justification(tool_name, tool_args);
    description_lines.extend(build_permission_context_lines(
        approval_reason,
        shell_justification.as_deref(),
        justification,
    ));

    description_lines.push(choose_handling_line("this run"));
    let mut navigation_hint = if prompt_kind == ToolPermissionPromptKind::Mcp {
        APPROVAL_NAVIGATE_CANCEL.to_string()
    } else {
        APPROVAL_NAVIGATE_DENY.to_string()
    };
    if source_thread_label.is_some() {
        navigation_hint.push_str(" • o inspect source thread");
    }

    let options = build_tool_permission_options(prompt_kind, persistent_approval_target);
    let hotkeys = source_thread_label
        .map(|_| {
            vec![TransientHotkey {
                key: TransientHotkeyKey::Char('o'),
                action: TransientHotkeyAction::OpenSourceThread,
            }]
        })
        .unwrap_or_default();

    use vtcode_ui::tui::app::InlineListSelection;
    let default_selection = InlineListSelection::ToolApproval(true);
    if hitl_notification_bell
        && let Err(err) = send_global_notification(NotificationEvent::PermissionPrompt {
            title: "VT Code approval required".to_string(),
            message: format!("Review the permission prompt for tool `{tool_name}`."),
        })
        .await
    {
        tracing::debug!(error = %err, "Failed to emit HITL notification");
    }
    let _placeholder_guard = PlaceholderGuard::new(handle, default_placeholder);
    let outcome = show_overlay_and_wait(
        handle,
        session,
        TransientRequest::List(ListOverlayRequest {
            title: "Tool Permission Required".to_string(),
            lines: description_lines,
            footer_hint: Some(navigation_hint),
            items: options,
            selected: Some(default_selection),
            search: None,
            hotkeys,
        }),
        ctrl_c_state,
        ctrl_c_notify,
        |submission| match submission {
            TransientSubmission::Hotkey(TransientHotkeyAction::OpenSourceThread) => {
                handle.set_input("/agent".to_string());
                let _ = renderer.line(
                    vtcode_core::utils::ansi::MessageStyle::Info,
                    "Switched focus to thread selector command. Run /agent to inspect the source thread, then retry the tool action.",
                );
                Some(HitlDecision::DeniedOnce)
            }
            TransientSubmission::Selection(InlineListSelection::ToolApproval(true)) => {
                Some(HitlDecision::Approved)
            }
            TransientSubmission::Selection(InlineListSelection::ToolApprovalSession) => {
                Some(HitlDecision::ApprovedSession)
            }
            TransientSubmission::Selection(InlineListSelection::ToolApprovalPermanent) => {
                Some(HitlDecision::ApprovedPermanent)
            }
            TransientSubmission::Selection(InlineListSelection::ToolApprovalDenyOnce) => {
                Some(HitlDecision::DeniedOnce)
            }
            TransientSubmission::Selection(InlineListSelection::ToolApproval(false)) => {
                Some(HitlDecision::Denied)
            }
            TransientSubmission::Selection(_) => Some(HitlDecision::Denied),
            _ => None,
        },
    )
    .await?;

    match outcome {
        OverlayWaitOutcome::Submitted(decision) => Ok(decision),
        OverlayWaitOutcome::Cancelled | OverlayWaitOutcome::Deferred => Ok(cancelled_prompt_decision(prompt_kind)),
        OverlayWaitOutcome::Interrupted => Ok(HitlDecision::Interrupt),
        OverlayWaitOutcome::Exit => Ok(HitlDecision::Exit),
    }
}

/// Show a HITL prompt for a tool that was denied by policy config.
/// Offers an "Enable tool" option that updates the tool policy to Allow,
/// and a "Deny Once" option to skip this invocation.
pub(super) async fn prompt_policy_denied_tool<S: UiSession + ?Sized>(
    tool_name: &str,
    tool_args: Option<&Value>,
    diagnostic: Option<Value>,
    _renderer: &mut AnsiRenderer,
    handle: &InlineHandle,
    ctrl_c_state: &Arc<CtrlCState>,
    ctrl_c_notify: &Arc<Notify>,
    session: &mut S,
) -> Result<HitlDecision> {
    use vtcode_ui::tui::app::{InlineListItem, InlineListSelection};

    let mut description_lines = vec![
        format!("Tool: {tool_name}"),
        "## Policy".to_string(),
        "This tool is currently denied by tool policy configuration.".to_string(),
    ];

    if let Some(url) = web_fetch_approval_url(tool_name, tool_args) {
        description_lines.push(format!("URL: {url}"));
    }

    if let Some(diag) = &diagnostic {
        if let Some(impact) = diag["impact"].as_str() {
            push_context_line(&mut description_lines, "Impact", impact);
        }
        if let Some(fix_action) = diag["fix"]["action"].as_str() {
            push_context_line(&mut description_lines, "Fix", fix_action);
        }
        if let Some(files) = diag["fix"]["files"].as_array() {
            for file in files {
                if let Some(f) = file.as_str() {
                    description_lines.push(format!("  - {f}"));
                }
            }
        }
    }

    description_lines.push(choose_handling_line("this run"));

    let options = vec![
        InlineListItem {
            title: "Enable tool".to_string(),
            subtitle: Some("Allow and continue execution".to_string()),
            badge: Some("Fix Policy".to_string()),
            indent: 0,
            selection: Some(InlineListSelection::ToolApprovalEnable),
            search_value: Some("enable allow fix policy continue yes 1".to_string()),
        },
        InlineListItem {
            title: "".to_string(),
            subtitle: None,
            badge: None,
            indent: 0,
            selection: None,
            search_value: None,
        },
        InlineListItem {
            title: "Deny Once".to_string(),
            subtitle: Some("Ask again next time".to_string()),
            badge: None,
            indent: 0,
            selection: Some(InlineListSelection::ToolApprovalDenyOnce),
            search_value: Some("deny no reject skip once 2".to_string()),
        },
    ];

    let navigation_hint = APPROVAL_NAVIGATE_DENY.to_string();

    let request = ListOverlayRequest {
        title: format!("Tool Policy: {tool_name}"),
        lines: description_lines,
        footer_hint: Some(navigation_hint),
        items: options,
        selected: None,
        search: None,
        hotkeys: Vec::new(),
    };

    let overlay = TransientRequest::List(request);

    let result =
        show_overlay_and_wait(handle, session, overlay, ctrl_c_state, ctrl_c_notify, |submission| match submission {
            TransientSubmission::Selection(InlineListSelection::ToolApprovalEnable) => Some(HitlDecision::Enable),
            TransientSubmission::Selection(InlineListSelection::ToolApprovalDenyOnce) => Some(HitlDecision::DeniedOnce),
            TransientSubmission::Selection(_) => Some(HitlDecision::DeniedOnce),
            _ => None,
        })
        .await?;

    match result {
        OverlayWaitOutcome::Submitted(decision) => Ok(decision),
        OverlayWaitOutcome::Cancelled | OverlayWaitOutcome::Deferred => Ok(HitlDecision::DeniedOnce),
        OverlayWaitOutcome::Interrupted => Ok(HitlDecision::Interrupt),
        OverlayWaitOutcome::Exit => Ok(HitlDecision::Exit),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use vtcode_core::config::constants::tools;

    use super::{
        ToolPermissionPromptKind, build_permission_context_lines, build_tool_permission_options,
        cancelled_prompt_decision, compact_justification_lines, extract_shell_approval_command_prefix_words,
        extract_shell_approval_justification, extract_shell_approval_scope_signature, extract_shell_command_text,
        extract_shell_persistent_approval_prefix_rule, format_command_preview_lines, lowercase_first_action,
        push_capped_context_line, render_shell_persistent_approval_prefix_entry, shell_allows_persistent_decisions,
        shell_command_preview_lines, shell_permission_cache_suffix, tool_permission_header_lines,
        tool_permission_prompt_kind, truncate_arg_preview,
    };
    use crate::agent::runloop::unified::tool_routing::shell_approval::PersistentApprovalTarget;

    #[test]
    fn command_session_extracts_command_when_action_is_run() {
        let args = json!({
            "action": "run",
            "command": "cargo check"
        });
        let command = extract_shell_command_text(tools::UNIFIED_EXEC, Some(&args));
        assert_eq!(command.as_deref(), Some("cargo check"));
    }

    #[test]
    fn command_session_extracts_cmd_alias_when_action_is_inferred() {
        let args = json!({
            "cmd": "cargo check"
        });
        let command = extract_shell_command_text(tools::UNIFIED_EXEC, Some(&args));
        assert_eq!(command.as_deref(), Some("cargo check"));
    }

    #[test]
    fn command_session_extracts_indexed_command_parts_when_action_is_inferred() {
        let args = json!({
            "command.0": "cargo",
            "command.1": "check"
        });
        let command = extract_shell_command_text(tools::UNIFIED_EXEC, Some(&args));
        assert_eq!(command.as_deref(), Some("cargo check"));
    }

    #[test]
    fn command_session_ignores_non_run_actions() {
        let args = json!({
            "action": "poll",
            "session_id": "run-123"
        });
        let command = extract_shell_command_text(tools::UNIFIED_EXEC, Some(&args));
        assert_eq!(command, None);
    }

    #[test]
    fn arg_preview_truncates_unicode_safely() {
        let value = "an’t ".repeat(20);
        let truncated = truncate_arg_preview(&value);
        assert!(truncated.ends_with("..."));
        assert!(truncated.chars().count() <= 60);
    }

    #[test]
    fn shell_runs_disable_persistent_decisions() {
        let args = json!({
            "action": "run",
            "command": "echo hi",
            "sandbox_permissions": "with_additional_permissions"
        });
        assert!(!shell_allows_persistent_decisions(tools::UNIFIED_EXEC, Some(&args)));
    }

    #[test]
    fn non_shell_command_session_actions_keep_persistent_decisions() {
        let args = json!({
            "action": "poll",
            "session_id": "run-123",
            "sandbox_permissions": "with_additional_permissions"
        });
        assert!(shell_allows_persistent_decisions(tools::UNIFIED_EXEC, Some(&args)));
    }

    #[test]
    fn cache_suffix_includes_permissions_for_shell_run() {
        let plain = json!({
            "command": "echo hi"
        });
        let with_permissions = json!({
            "command": "echo hi",
            "sandbox_permissions": "with_additional_permissions",
            "additional_permissions": {
                "fs_write": ["/tmp/demo.txt"]
            }
        });

        let plain_key = shell_permission_cache_suffix("shell", Some(&plain));
        let permissioned_key = shell_permission_cache_suffix("shell", Some(&with_permissions));

        assert_ne!(plain_key, permissioned_key);
        assert!(
            permissioned_key
                .as_deref()
                .unwrap_or_default()
                .contains("with_additional_permissions")
        );
    }

    #[test]
    fn cache_suffix_normalizes_use_default_with_additional_permissions() {
        let args = json!({
            "command": "echo hi",
            "sandbox_permissions": "use_default",
            "additional_permissions": {
                "fs_write": ["/tmp/demo.txt"]
            }
        });

        let permissioned_key = shell_permission_cache_suffix("shell", Some(&args));

        assert_eq!(
            permissioned_key.as_deref(),
            Some(
                "echo hi|sandbox_permissions=\"with_additional_permissions\"|additional_permissions={\"fs_write\":[\"/tmp/demo.txt\"]}"
            )
        );
    }

    #[test]
    fn cache_suffix_ignores_empty_additional_permissions() {
        let args = json!({
            "command": "echo hi",
            "sandbox_permissions": "use_default",
            "additional_permissions": {
                "fs_read": [],
                "fs_write": []
            }
        });

        let permissioned_key = shell_permission_cache_suffix("shell", Some(&args));

        assert_eq!(permissioned_key.as_deref(), Some("echo hi"));
    }

    #[test]
    fn shell_approval_justification_is_extracted_for_run_actions() {
        let args = json!({
            "action": "run",
            "command": "cargo build",
            "justification": "Do you want to build the project outside the sandbox?"
        });

        let justification = extract_shell_approval_justification(tools::UNIFIED_EXEC, Some(&args));
        assert_eq!(justification.as_deref(), Some("Do you want to build the project outside the sandbox?"));
    }

    #[test]
    fn shell_approval_justification_ignores_non_run_actions() {
        let args = json!({
            "action": "poll",
            "session_id": "run-123",
            "justification": "ignored"
        });

        let justification = extract_shell_approval_justification(tools::UNIFIED_EXEC, Some(&args));
        assert_eq!(justification, None);
    }

    #[test]
    fn shell_persistent_prefix_rule_requires_matching_command_prefix() {
        let args = json!({
            "action": "run",
            "command": "cargo test -p vtcode",
            "prefix_rule": ["cargo", "build"]
        });

        let prefix_rule = extract_shell_persistent_approval_prefix_rule(tools::UNIFIED_EXEC, Some(&args));
        assert_eq!(prefix_rule, None);
    }

    #[test]
    fn shell_persistent_prefix_rule_rejects_compound_commands() {
        let args = json!({
            "action": "run",
            "command": "cargo test && cargo fmt",
            "prefix_rule": ["cargo", "test"]
        });

        let prefix_rule = extract_shell_persistent_approval_prefix_rule(tools::UNIFIED_EXEC, Some(&args));
        assert_eq!(prefix_rule, None);
    }

    #[test]
    fn shell_approval_prefix_text_normalizes_simple_commands() {
        let args = json!({
            "action": "run",
            "command": ["cargo", "test", "-p", "vtcode"]
        });

        let command = extract_shell_approval_command_prefix_words(tools::UNIFIED_EXEC, Some(&args));
        assert_eq!(
            command,
            Some(vec![
                "cargo".to_string(),
                "test".to_string(),
                "-p".to_string(),
                "vtcode".to_string()
            ])
        );
    }

    #[test]
    fn shell_approval_scope_signature_tracks_requested_permissions() {
        let args = json!({
            "action": "run",
            "command": "cargo test",
            "sandbox_permissions": "require_escalated"
        });

        let scope = extract_shell_approval_scope_signature(tools::UNIFIED_EXEC, Some(&args));
        assert_eq!(scope.as_deref(), Some("sandbox_permissions=\"require_escalated\"|additional_permissions=null"));
    }

    #[test]
    fn shell_approval_scope_signature_normalizes_implicit_additional_permissions() {
        let args = json!({
            "action": "run",
            "command": "cargo test",
            "sandbox_permissions": "use_default",
            "additional_permissions": {
                "fs_write": ["/tmp/demo.txt"]
            }
        });

        let scope = extract_shell_approval_scope_signature(tools::UNIFIED_EXEC, Some(&args));
        assert_eq!(
            scope.as_deref(),
            Some(
                "sandbox_permissions=\"with_additional_permissions\"|additional_permissions={\"fs_write\":[\"/tmp/demo.txt\"]}"
            )
        );
    }

    #[test]
    fn shell_persistent_approval_entry_includes_scope() {
        let args = json!({
            "action": "run",
            "command": "cargo test -p vtcode",
            "prefix_rule": ["cargo", "test"],
            "sandbox_permissions": "require_escalated"
        });

        let entry = render_shell_persistent_approval_prefix_entry(
            tools::UNIFIED_EXEC,
            Some(&args),
            &["cargo".to_string(), "test".to_string()],
        );
        assert_eq!(
            entry.as_deref(),
            Some("cargo test|sandbox_permissions=\"require_escalated\"|additional_permissions=null")
        );
    }

    #[test]
    fn canonical_mcp_tools_use_mcp_prompt_kind() {
        assert_eq!(tool_permission_prompt_kind("mcp::calendar::list_events"), ToolPermissionPromptKind::Mcp);
    }

    #[test]
    fn model_visible_mcp_tools_use_mcp_prompt_kind() {
        assert_eq!(tool_permission_prompt_kind("mcp__calendar__list_events"), ToolPermissionPromptKind::Mcp);
    }

    #[test]
    fn non_mcp_tools_keep_standard_prompt_kind() {
        assert_eq!(tool_permission_prompt_kind(tools::UNIFIED_EXEC), ToolPermissionPromptKind::Standard);
    }

    #[test]
    fn mcp_prompt_uses_cancel_without_persistent_deny() {
        let titles =
            build_tool_permission_options(ToolPermissionPromptKind::Mcp, Some(&PersistentApprovalTarget::ToolLevel))
                .into_iter()
                .map(|item| item.title)
                .collect::<Vec<_>>();
        assert!(titles.iter().any(|title| title == "Cancel"));
        assert!(!titles.iter().any(|title| title == "Always Deny"));
    }

    #[test]
    fn standard_prompt_keeps_persistent_deny() {
        let titles = build_tool_permission_options(
            ToolPermissionPromptKind::Standard,
            Some(&PersistentApprovalTarget::ToolLevel),
        )
        .into_iter()
        .map(|item| item.title)
        .collect::<Vec<_>>();
        assert!(titles.iter().any(|title| title == "Deny Once"));
        assert!(titles.iter().any(|title| title == "Always Deny"));
    }

    #[test]
    fn shell_prompt_offers_policy_cache_option_for_exact_invocation() {
        let titles = build_tool_permission_options(
            ToolPermissionPromptKind::Standard,
            Some(&PersistentApprovalTarget::ExactInvocation {
                display_label: "command `cargo clippy`".to_string(),
            }),
        )
        .into_iter()
        .map(|item| item.title)
        .collect::<Vec<_>>();
        assert!(titles.iter().any(|title| title == "Always approve and save to policy cache"));
        assert!(!titles.iter().any(|title| title == "Always Deny"));
    }

    #[test]
    fn shell_header_uses_permission_intro_without_tool_jargon() {
        let lines = tool_permission_header_lines("exec_command", "python3 -c 'truncated…'", true, None);
        assert_eq!(lines, vec!["The agent wants to run a shell command and needs your approval."]);
    }

    #[test]
    fn header_keeps_action_summary_for_diff_preview_file_identity() {
        let lines = tool_permission_header_lines("edit_file", "Edit file src/main.rs", false, None);
        assert_eq!(lines, vec!["The agent wants to edit file src/main.rs and needs your approval."]);
    }

    #[test]
    fn header_lists_source_above_action_summary() {
        let lines = tool_permission_header_lines("edit_file", "Edit file src/main.rs", false, Some("agent-1"));
        assert_eq!(
            lines,
            vec![
                "The agent wants to edit file src/main.rs and needs your approval.",
                "Requested from: agent-1"
            ]
        );
    }

    #[test]
    fn header_falls_back_to_tool_name_when_action_is_empty() {
        let lines = tool_permission_header_lines("exec_command", "   ", false, None);
        assert_eq!(lines, vec!["The agent wants to use exec_command and needs your approval."]);
    }

    #[test]
    fn header_moves_mcp_prefix_to_trailing_via_mcp() {
        let lines = tool_permission_header_lines("mcp__calendar__list_events", "MCP Search code for docs", false, None);
        assert_eq!(lines, vec!["The agent wants to search code for docs via MCP and needs your approval."]);
    }

    #[test]
    fn lowercase_first_action_preserves_leading_acronyms() {
        assert_eq!(lowercase_first_action("Edit file src/main.rs"), "edit file src/main.rs");
        assert_eq!(lowercase_first_action("MCP Search code"), "MCP Search code");
        assert_eq!(lowercase_first_action(""), "");
    }

    #[test]
    fn permission_context_lines_fall_back_without_any_reason() {
        let lines = build_permission_context_lines(None, None, None);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].starts_with("No additional details were provided."));
    }

    #[test]
    fn permission_context_lines_fall_back_when_reason_is_blank() {
        let lines = build_permission_context_lines(Some("   \n  "), None, None);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].starts_with("No additional details were provided."));
    }

    #[test]
    fn permission_context_lines_dedupes_shell_goal_with_justification_goal() {
        let just = vtcode_core::tools::ToolJustification {
            tool_name: "exec_command".to_string(),
            reason: "List the workspace".to_string(),
            expected_outcome: None,
            risk_level: "Low".to_string(),
            timestamp: "now".to_string(),
        };
        let lines = build_permission_context_lines(None, Some("List the workspace"), Some(&just));
        let goals = lines.iter().filter(|line| line.starts_with(super::AGENT_GOAL_LABEL)).count();
        assert_eq!(goals, 1, "shell and ledger goals must not duplicate: {lines:?}");
        assert!(lines.iter().any(|line| line == "Risk: Low"));
    }

    #[test]
    fn permission_context_lines_keeps_risk_when_goal_comes_from_approval_reason() {
        let just = vtcode_core::tools::ToolJustification {
            tool_name: "exec_command".to_string(),
            reason: "Ledger goal".to_string(),
            expected_outcome: None,
            risk_level: "High".to_string(),
            timestamp: "now".to_string(),
        };
        let lines = build_permission_context_lines(Some("Explicit reason"), None, Some(&just));
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with(&format!("{}: Explicit reason", super::AGENT_GOAL_LABEL)));
        assert_eq!(lines[1], "Risk: High");
    }

    #[test]
    fn mcp_prompt_cancellation_is_non_persistent() {
        assert_eq!(cancelled_prompt_decision(ToolPermissionPromptKind::Mcp), super::HitlDecision::DeniedOnce);
    }

    #[test]
    fn capped_context_line_keeps_first_logical_row_only() {
        let mut lines = Vec::new();
        push_capped_context_line(&mut lines, "Reason", "  \nFirst reason\nSecond reason\n", 160);
        assert_eq!(lines, vec!["Reason: First reason"]);

        let mut blank = Vec::new();
        push_capped_context_line(&mut blank, "Reason", "   \n  ", 160);
        assert!(blank.is_empty());
    }

    #[test]
    fn compact_justification_lines_keep_reason_and_risk_only() {
        let just = vtcode_core::tools::ToolJustification {
            tool_name: "exec_command".to_string(),
            reason: "Build the project\nwith more detail on a second line".to_string(),
            expected_outcome: Some("Binary compiles\nplus extra".to_string()),
            risk_level: "High".to_string(),
            timestamp: "now".to_string(),
        };
        let lines = compact_justification_lines(&just);
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with("What the agent is trying to do: Build the project"));
        assert!(!lines[0].contains('\n'));
        assert!(!lines.iter().any(|line| line.starts_with("Expected:")));
        assert_eq!(lines[1], "Risk: High");
    }

    #[test]
    fn compact_justification_lines_truncate_long_reason() {
        let just = vtcode_core::tools::ToolJustification {
            tool_name: "exec_command".to_string(),
            reason: "x".repeat(400),
            expected_outcome: None,
            risk_level: String::new(),
            timestamp: "now".to_string(),
        };
        let lines = compact_justification_lines(&just);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].chars().count() <= super::AGENT_GOAL_LABEL.len() + 2 + super::MAX_CONTEXT_LINE_CHARS);
    }

    #[test]
    fn permanent_option_subtitle_truncates_long_command_label() {
        let long_label = format!("command `python3 -c '{}'`", "y".repeat(200));
        let items = build_tool_permission_options(
            ToolPermissionPromptKind::Standard,
            Some(&PersistentApprovalTarget::ExactInvocation { display_label: long_label }),
        );
        let permanent = items
            .iter()
            .find(|item| item.title == "Always approve and save to policy cache")
            .expect("permanent option");
        let subtitle = permanent.subtitle.as_deref().expect("subtitle");
        assert!(subtitle.contains('…'), "long label should be truncated, got: {subtitle}");
        assert!(subtitle.chars().count() <= "Remember  in this workspace".len() + super::MAX_APPROVAL_LABEL_CHARS + 8);
    }

    #[test]
    fn command_preview_distinguishes_same_prefix_different_dangerous_suffixes() {
        let shared = "python3 -c 'print(1)' --output ".to_string() + &"safe-prefix/".repeat(12);
        let first = format_command_preview_lines(vec![format!("{shared}report.txt")]);
        let second = format_command_preview_lines(vec![format!("{shared}../../critical.txt")]);
        assert_ne!(first, second);
        assert!(first[0].contains("report.txt"), "tail missing from {first:?}");
        assert!(second[0].contains("../../critical.txt"), "dangerous tail missing from {second:?}");
        assert!(first[0].contains('…') && second[0].contains('…'));
    }

    #[test]
    fn command_preview_preserves_short_unicode_command() {
        let preview = format_command_preview_lines(vec!["printf 'xin chào 世界'".to_string()]);
        assert_eq!(preview, vec!["`printf 'xin chào 世界'`"]);
    }

    #[test]
    fn multiline_command_preview_keeps_head_tail_and_omission_count() {
        let lines = (1..=12).map(|line| format!("line {line}")).collect();
        let preview = format_command_preview_lines(lines);
        assert_eq!(preview.len(), 8);
        assert_eq!(preview[0], "`line 1`");
        assert_eq!(preview[4], "`line 5`");
        assert_eq!(preview[5], "… +5 more lines (full command runs on approval)");
        assert_eq!(preview[6], "`line 11`");
        assert_eq!(preview[7], "`line 12`");
    }

    #[test]
    fn shell_command_preview_uses_canonical_extractor_for_aliases() {
        for (tool_name, args) in [
            ("exec_command", json!({"cmd": "cargo check --locked"})),
            (tools::UNIFIED_EXEC, json!({"action": "run", "command": "cargo nextest run"})),
            ("container.exec", json!({"command": "git status --short"})),
        ] {
            let preview = shell_command_preview_lines(tool_name, Some(&args)).expect("run alias has command preview");
            assert_eq!(preview.len(), 1);
            assert!(!preview[0].is_empty());
        }
        assert!(
            shell_command_preview_lines(tools::UNIFIED_EXEC, Some(&json!({"action": "poll", "session_id": "run-1"})))
                .is_none()
        );
    }
}
