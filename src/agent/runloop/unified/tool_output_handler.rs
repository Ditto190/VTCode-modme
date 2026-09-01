use crate::agent::runloop::git::normalize_workspace_path;
use crate::agent::runloop::mcp_events::McpPanelState;
use crate::agent::runloop::unified::state::SessionStats;
use anyhow::Result;
use std::collections::BTreeSet;
use std::path::Path;
use std::path::PathBuf;
use vtcode_commons::paths::ensure_path_within_workspace_resolved;
use vtcode_core::config::ToolDisplayMode;
use vtcode_core::config::constants::tools;
use vtcode_core::config::loader::VTCodeConfig;
use vtcode_core::tools::tool_intent;
use vtcode_core::utils::ansi::AnsiRenderer;
use vtcode_core::utils::ansi::MessageStyle;
use vtcode_core::utils::style_helpers::ColorPalette;
use vtcode_core::utils::transcript;
use vtcode_ui::tui::app::{InlineHandle, InlineMessageKind, InlineSegment, InlineTextStyle};

use crate::agent::runloop::unified::run_loop_context::RunLoopContext;
use crate::agent::runloop::unified::tool_pipeline::{
    ToolDisplayStatus, ToolExecutionStatus, ToolPipelineOutcome, renders_pty_command_header, streams_pty_output,
};
use vtcode_commons::canonicalize;

fn record_mcp_success_event(mcp_panel_state: &mut McpPanelState, tool_name: &str, args_val: &serde_json::Value) {
    let mut mcp_event = crate::agent::runloop::mcp_events::McpEvent::new(
        "mcp".to_string(),
        tool_name.to_string(),
        Some(args_val.to_string()),
    );
    mcp_event.success(None);
    mcp_panel_state.add_event(mcp_event);
}

fn collect_modified_files(modified_files: &[String]) -> Vec<PathBuf> {
    modified_files.iter().map(PathBuf::from).collect()
}

fn collect_instruction_activity_paths(
    workspace_root: &Path,
    args_val: &serde_json::Value,
    output: &serde_json::Value,
    modified_files: &[String],
) -> Vec<PathBuf> {
    let canonical_workspace = canonicalize(workspace_root).unwrap_or_else(|_| workspace_root.to_path_buf());
    let mut paths = BTreeSet::new();
    for modified in modified_files {
        push_activity_path(workspace_root, &canonical_workspace, modified, &mut paths);
    }
    collect_paths_from_value(workspace_root, &canonical_workspace, Some("args"), args_val, &mut paths);
    collect_paths_from_value(workspace_root, &canonical_workspace, Some("output"), output, &mut paths);
    paths.into_iter().collect()
}

fn collect_paths_from_value(
    workspace_root: &Path,
    canonical_workspace: &Path,
    key: Option<&str>,
    value: &serde_json::Value,
    paths: &mut BTreeSet<PathBuf>,
) {
    match value {
        serde_json::Value::String(text) => {
            if key.is_some_and(path_like_key) {
                push_activity_path(workspace_root, canonical_workspace, text, paths);
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                collect_paths_from_value(workspace_root, canonical_workspace, key, value, paths);
            }
        }
        serde_json::Value::Object(map) => {
            for (child_key, child_value) in map {
                collect_paths_from_value(
                    workspace_root,
                    canonical_workspace,
                    Some(child_key.as_str()),
                    child_value,
                    paths,
                );
            }
        }
        serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::Number(_) => {}
    }
}

fn path_like_key(key: &str) -> bool {
    matches!(
        key,
        "path"
            | "paths"
            | "file"
            | "files"
            | "file_path"
            | "file_paths"
            | "cwd"
            | "workdir"
            | "directory"
            | "directories"
            | "root"
            | "workspace"
    )
}

fn push_activity_path(workspace_root: &Path, canonical_workspace: &Path, raw: &str, paths: &mut BTreeSet<PathBuf>) {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.contains("://") || trimmed.starts_with("untitled:") {
        return;
    }

    let normalized = normalize_workspace_path(workspace_root, Path::new(trimmed));
    if normalized.starts_with(canonical_workspace) || normalized.starts_with(workspace_root) {
        paths.insert(normalized);
    }
}

fn is_run_pty_tool(name: &str, args_val: &serde_json::Value) -> bool {
    renders_pty_command_header(name, args_val)
}

fn is_command_output_call(name: &str, args_val: &serde_json::Value) -> bool {
    name == tools::EXECUTE_CODE
        || tool_intent::is_command_run_tool_call(name, args_val)
        || is_run_pty_tool(name, args_val)
}

fn compact_run_completion_line(output: &serde_json::Value, status: ToolDisplayStatus) -> Option<String> {
    if let Some(exit_code) = output.get("exit_code").and_then(serde_json::Value::as_i64) {
        if matches!(status, ToolDisplayStatus::Success) && exit_code == 0 {
            return Some("✓ run completed (exit code: 0)".to_string());
        }
        if matches!(status, ToolDisplayStatus::Warning) && exit_code == 0 {
            return Some("⚠ run completed with warnings (exit code: 0)".to_string());
        }
        return Some(format!("✗ run error, exit code: {exit_code}"));
    }

    if output.get("is_exited").and_then(serde_json::Value::as_bool) == Some(true) {
        if matches!(status, ToolDisplayStatus::Success) {
            return Some("✓ done".to_string());
        }
        if matches!(status, ToolDisplayStatus::Warning) {
            return Some("⚠ done with warnings".to_string());
        }
        return Some("✗ failed".to_string());
    }

    match status {
        ToolDisplayStatus::Failure => Some("✗ failed".to_string()),
        ToolDisplayStatus::Warning => Some("⚠ completed with warnings".to_string()),
        ToolDisplayStatus::Success => None,
    }
}

fn is_git_diff_payload(output: &serde_json::Value) -> bool {
    output
        .get("content_type")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|content_type| content_type == "git_diff")
}

fn has_renderable_stream_content(output: &serde_json::Value) -> bool {
    ["output", "stdout", "stderr"].iter().any(|key| {
        output
            .get(*key)
            .and_then(serde_json::Value::as_str)
            .is_some_and(|s| !s.trim().is_empty())
    })
}

fn is_task_tracker_tool(name: &str) -> bool {
    matches!(name, tools::TASK_TRACKER)
}

fn task_tracker_block_lines(output: &serde_json::Value) -> Vec<String> {
    crate::agent::runloop::tool_output::tracker_view_lines(output)
}

fn task_tracker_block_segments(lines: &[String]) -> Vec<Vec<InlineSegment>> {
    let style = std::sync::Arc::new(InlineTextStyle::default());
    lines
        .iter()
        .map(|line| vec![InlineSegment { text: line.clone(), style: style.clone() }])
        .collect()
}

fn apply_task_tracker_block(
    handle: &InlineHandle,
    harness_state: &mut crate::agent::runloop::unified::run_loop_context::HarnessTurnState,
    lines: Vec<String>,
) {
    let replace_count = harness_state.replaceable_task_tracker_count();
    let segments = task_tracker_block_segments(&lines);

    if let Some(count) = replace_count {
        handle.replace_last(count, InlineMessageKind::Tool, segments);
        transcript::replace_last(count, &lines);
    } else {
        for (segments, plain_line) in segments.into_iter().zip(lines.iter()) {
            handle.append_line(InlineMessageKind::Tool, segments);
            transcript::append(plain_line);
        }
    }

    harness_state.remember_task_tracker_block(lines);
}

/// Extract the command string from tool call arguments.
fn extract_command_line(args: &serde_json::Value) -> Option<String> {
    args.get("command")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .or_else(|| args.get("raw_command").and_then(serde_json::Value::as_str).map(str::to_string))
        .or_else(|| args.get("cmd").and_then(serde_json::Value::as_str).map(str::to_string))
}

/// Record the tool-call summary line ("• Ran ...") to the transcript only.
fn record_summary_line(name: &str, args: &serde_json::Value, _output: &serde_json::Value, _command_success: bool) {
    let action_label = if tool_intent::is_command_run_tool_call(name, args) {
        "Run command"
    } else {
        name
    };
    let headline = if action_label == "Run command" {
        if let Some(cmd) = extract_command_line(args) {
            format!("Ran {cmd}")
        } else {
            "Ran command".to_string()
        }
    } else {
        format!("• {action_label}")
    };
    transcript::append(&headline);
}

fn contains_line_block(container: &str, candidate: &str) -> bool {
    let container_lines = container.lines().collect::<Vec<_>>();
    let candidate_lines = candidate.lines().collect::<Vec<_>>();
    !candidate_lines.is_empty()
        && candidate_lines.len() <= container_lines.len()
        && container_lines
            .windows(candidate_lines.len())
            .any(|window| window == candidate_lines.as_slice())
}

fn streams_are_aliases(left: &str, right: &str) -> bool {
    contains_line_block(left, right) || contains_line_block(right, left)
}

fn output_text<'a>(output: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    output
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim_end)
        .filter(|text| !text.trim().is_empty())
}

fn ordered_stream_texts(output: &serde_json::Value) -> Vec<&str> {
    let mut texts: Vec<&str> = Vec::new();
    for key in ["output", "stdout", "stderr"] {
        let Some(text) = output_text(output, key) else {
            continue;
        };
        if texts.iter().any(|existing| contains_line_block(existing, text)) {
            continue;
        }
        if let Some(index) = texts.iter().position(|existing| contains_line_block(text, existing)) {
            texts[index] = text;
        } else {
            texts.push(text);
        }
    }
    texts
}

#[derive(Clone, Copy)]
struct CanonicalOutputStream<'a> {
    label: Option<&'static str>,
    text: &'a str,
}

fn canonical_pipe_streams(output: &serde_json::Value) -> Vec<CanonicalOutputStream<'_>> {
    let merged = output_text(output, "output");
    let stdout = output_text(output, "stdout");
    let stderr = output_text(output, "stderr");
    let mut streams = Vec::new();

    if let Some(merged) = merged {
        let stdout_is_in_merged = stdout.is_some_and(|text| contains_line_block(merged, text));
        let stderr_is_in_merged = stderr.is_some_and(|text| contains_line_block(merged, text));

        // A combined `output` field is authoritative when it contains both
        // named streams. The named fields are aliases in that case and must
        // not be appended a second time.
        if stdout_is_in_merged && stderr_is_in_merged {
            streams.push(CanonicalOutputStream { label: None, text: merged });
            return streams;
        }

        // If `output` only mirrors one named stream, keep the merged text under
        // that source label so expanded content is not lost and the alias is
        // not appended a second time. Also handle a short `output` preview
        // nested inside a named stream by preferring the longer named value.
        let stdout_is_alias = stdout.is_some_and(|text| streams_are_aliases(merged, text));
        let stderr_is_alias = stderr.is_some_and(|text| streams_are_aliases(merged, text));
        if stdout_is_alias {
            let text = stdout.filter(|text| contains_line_block(text, merged)).unwrap_or(merged);
            streams.push(CanonicalOutputStream { label: Some("stdout"), text });
        } else if stderr_is_alias {
            let text = stderr.filter(|text| contains_line_block(text, merged)).unwrap_or(merged);
            streams.push(CanonicalOutputStream { label: Some("stderr"), text });
        } else {
            streams.push(CanonicalOutputStream { label: None, text: merged });
        }

        if let Some(stdout) = stdout
            && !stdout_is_alias
            && !streams_are_aliases(stdout, merged)
        {
            streams.push(CanonicalOutputStream { label: Some("stdout"), text: stdout });
        }
        if let Some(stderr) = stderr
            && !stderr_is_alias
            && !streams_are_aliases(stderr, merged)
        {
            streams.push(CanonicalOutputStream { label: Some("stderr"), text: stderr });
        }
        return streams;
    }

    if let Some(stdout) = stdout {
        streams.push(CanonicalOutputStream { label: Some("stdout"), text: stdout });
    }
    if let Some(stderr) = stderr {
        streams.push(CanonicalOutputStream { label: Some("stderr"), text: stderr });
    }
    streams
}

async fn load_complete_output(output: &serde_json::Value, workspace_root: Option<&Path>) -> Option<String> {
    if output.get("spool_path").is_some() {
        let spool_path = output
            .get("spool_path")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|path| !path.is_empty())?;
        let root = workspace_root?;
        let candidate = if Path::new(spool_path).is_absolute() {
            PathBuf::from(spool_path)
        } else {
            root.join(spool_path)
        };
        let resolved = match ensure_path_within_workspace_resolved(&candidate, root).await {
            Ok(path) => path,
            Err(error) => {
                tracing::warn!(path = %candidate.display(), %error, "Rejected tool output spool path");
                return None;
            }
        };
        return match tokio::fs::read_to_string(&resolved).await {
            Ok(content) => Some(content),
            Err(error) => {
                tracing::warn!(path = %resolved.display(), %error, "Failed to read tool output spool");
                None
            }
        };
    }

    let texts = ordered_stream_texts(output);
    (!texts.is_empty()).then(|| texts.join("\n"))
}

fn normalize_terminal_output_lines(capture: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut chars = capture.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\x1b' => match chars.next() {
                Some('[') => {
                    let mut params = String::new();
                    let final_byte = loop {
                        let Some(next) = chars.next() else {
                            break None;
                        };
                        if ('@'..='~').contains(&next) {
                            break Some(next);
                        }
                        params.push(next);
                    };

                    match final_byte {
                        // Clear-screen sequences mean that the earlier text
                        // was only a stale terminal frame, not command output
                        // that should remain in the readable viewer.
                        Some('J') if params.starts_with('2') || params.starts_with('3') => {
                            lines.clear();
                            current.clear();
                        }
                        // Erase the current line for the common progress-bar
                        // rewrite sequence. Styling and cursor movement are
                        // intentionally omitted from the plain-text viewer.
                        Some('K') if params.starts_with('2') => current.clear(),
                        _ => {}
                    }
                }
                Some(']') => {
                    // Skip OSC title/hyperlink sequences through BEL or ST.
                    while let Some(next) = chars.next() {
                        if next == '\x07' {
                            break;
                        }
                        if next == '\x1b' && chars.peek() == Some(&'\\') {
                            let _ = chars.next();
                            break;
                        }
                    }
                }
                Some(_) | None => {}
            },
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    let _ = chars.next();
                    lines.push(std::mem::take(&mut current));
                } else {
                    current.clear();
                }
            }
            '\n' => lines.push(std::mem::take(&mut current)),
            '\u{8}' => {
                let _ = current.pop();
            }
            _ => current.push(ch),
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

fn command_output_header(name: &str, args: &serde_json::Value, workspace_root: Option<&Path>) -> String {
    let command = extract_command_line(args)
        .map(|command| vtcode_commons::formatting::collapse_whitespace(&command))
        .filter(|command| !command.is_empty())
        .map(|command| {
            crate::agent::runloop::unified::tool_summary_helpers::relativize_command_paths(&command, workspace_root)
        });
    command
        .map(|command| format!("• Ran {command}"))
        .unwrap_or_else(|| format!("• Ran {name}"))
}

fn append_merged_output_lines(lines: &mut Vec<String>, output_lines: impl IntoIterator<Item = String>) {
    for (index, line) in output_lines.into_iter().enumerate() {
        if index == 0 {
            lines.push(format!("  └ {line}"));
        } else {
            lines.push(format!("    {line}"));
        }
    }
}

fn append_labeled_output_lines(lines: &mut Vec<String>, label: &str, output_lines: impl IntoIterator<Item = String>) {
    lines.push(format!("  {label}:"));
    for line in output_lines {
        lines.push(format!("    {line}"));
    }
}

fn append_viewer_status_line(lines: &mut Vec<String>, output: &serde_json::Value, status: ToolDisplayStatus) {
    if !matches!(status, ToolDisplayStatus::Success)
        && let Some(completion) = compact_run_completion_line(output, status)
    {
        lines.push(format!("    {completion}"));
    }
}

fn build_merged_command_output_lines(
    name: &str,
    args: &serde_json::Value,
    capture: &str,
    workspace_root: Option<&Path>,
    output: &serde_json::Value,
    status: ToolDisplayStatus,
) -> Vec<String> {
    let output_lines = normalize_terminal_output_lines(capture);
    let mut lines = vec![command_output_header(name, args, workspace_root)];
    append_merged_output_lines(&mut lines, output_lines);
    if let Some(note) = output_text(output, "critical_note") {
        lines.push(format!("    {note}"));
    }
    append_viewer_status_line(&mut lines, output, status);
    lines
}

fn build_pipe_command_output_lines(
    name: &str,
    args: &serde_json::Value,
    output: &serde_json::Value,
    workspace_root: Option<&Path>,
    status: ToolDisplayStatus,
) -> Vec<String> {
    let mut lines = vec![command_output_header(name, args, workspace_root)];
    for stream in canonical_pipe_streams(output) {
        let output_lines = normalize_terminal_output_lines(stream.text);
        if output_lines.is_empty() {
            continue;
        }
        if let Some(label) = stream.label {
            append_labeled_output_lines(&mut lines, label, output_lines);
        } else {
            append_merged_output_lines(&mut lines, output_lines);
        }
    }
    if let Some(note) = output_text(output, "critical_note") {
        lines.push(format!("    {note}"));
    }
    append_viewer_status_line(&mut lines, output, status);
    lines
}

async fn render_tool_output_common(
    renderer: &mut AnsiRenderer,
    handle: &InlineHandle,
    name: &str,
    args_val: &serde_json::Value,
    output: &serde_json::Value,
    command_success: bool,
    vt_config: Option<&VTCodeConfig>,
    workspace_root: Option<&Path>,
) -> Result<()> {
    let inline_run_tool = renderer.supports_inline_ui() && streams_pty_output(name, args_val);
    let git_diff_payload = is_git_diff_payload(output);
    let status = ToolDisplayStatus::from_command_output(output, command_success);
    let has_spool_path = output.get("spool_path").is_some();
    let complete_capture = if renderer.supports_inline_ui()
        && is_command_output_call(name, args_val)
        && (inline_run_tool || has_spool_path)
    {
        load_complete_output(output, workspace_root).await
    } else {
        None
    };

    // For streamed inline PTY tools: the pre-execution Pty block already
    // shows "• Ran ..." and output. Skip duplicating the output in the TUI,
    // but still record the summary and complete output for the viewer.
    if inline_run_tool && !git_diff_payload {
        // Record the command summary to transcript (TUI already showed it via PTY block)
        record_summary_line(name, args_val, output, command_success);

        // Prefer the complete PTY spool (or the complete inline result) for
        // the session-local tool-output viewer. The live PTY block remains
        // bounded separately.
        if let Some(capture) = complete_capture.as_deref() {
            let viewer_lines =
                build_merged_command_output_lines(name, args_val, capture, workspace_root, output, status);
            handle.record_tool_output(viewer_lines);
        } else {
            // A rejected or unavailable spool must not fall back to a
            // potentially untrusted path. Keep the command call visible in
            // the viewer while retaining fail-closed spool handling.
            handle.record_tool_output(if has_spool_path {
                build_merged_command_output_lines(name, args_val, "", workspace_root, output, status)
            } else {
                build_pipe_command_output_lines(name, args_val, output, workspace_root, status)
            });
        }

        if let Some(note) = output_text(output, "critical_note") {
            renderer.line(MessageStyle::ToolError, note)?;
            transcript::append(note);
        }

        if !has_renderable_stream_content(output) && matches!(status, ToolDisplayStatus::Success) {
            if renderer.tool_display_mode() != ToolDisplayMode::Compact {
                renderer.line(MessageStyle::Info, "(no output)")?;
            }
            return Ok(());
        }

        // Send completion as a status line only when the command needs
        // attention; on success the colored header bullet is sufficient.
        if !matches!(status, ToolDisplayStatus::Success) {
            if let Some(completion) = compact_run_completion_line(output, status) {
                let indented = format!("    {}", completion);
                renderer.line(MessageStyle::Status, &indented)?;
                transcript::append(&completion);
            }
        }
        return Ok(());
    }

    if renderer.supports_inline_ui() && is_command_output_call(name, args_val) {
        let viewer_lines = if inline_run_tool || has_spool_path {
            complete_capture.map_or_else(
                || build_merged_command_output_lines(name, args_val, "", workspace_root, output, status),
                |capture| build_merged_command_output_lines(name, args_val, &capture, workspace_root, output, status),
            )
        } else {
            build_pipe_command_output_lines(name, args_val, output, workspace_root, status)
        };
        handle.record_tool_output(viewer_lines);
    }

    // Render the summary header for non-streamed tools.
    // (streamed PTY tools with git_diff_payload also skip summary here,
    //  falling through to the full render_tool_output path.)
    if !(inline_run_tool && git_diff_payload) {
        let stream_label =
            crate::agent::runloop::unified::tool_summary::stream_label_from_output(output, command_success);
        let summary_ctx = crate::agent::runloop::unified::tool_summary::ToolSummaryRenderContext { workspace_root };
        let bullet_color = status.color(ColorPalette::default());
        if matches!(status, ToolDisplayStatus::Success) {
            crate::agent::runloop::unified::tool_summary::render_tool_call_summary(
                renderer,
                name,
                args_val,
                stream_label,
                &summary_ctx,
                bullet_color,
            )?;
        } else {
            crate::agent::runloop::unified::tool_summary::render_expanded_tool_call_summary(
                renderer,
                name,
                args_val,
                stream_label,
                &summary_ctx,
                bullet_color,
            )?;
        }
    }

    crate::agent::runloop::tool_output::render_tool_output(renderer, Some(name), output, vt_config).await
}

fn render_error_common(renderer: &mut AnsiRenderer, name: &str, error: &str, error_type: &str) -> Result<()> {
    let err_msg = format!("Tool '{name}' {error_type}: {error}");
    renderer.line(MessageStyle::Error, &err_msg)?;
    Ok(())
}

#[derive(Default)]
struct OutcomeState {
    turn_modified_files: Vec<PathBuf>,
    last_tool_stdout: Option<String>,
}

impl OutcomeState {
    fn into_tuple(self) -> (Vec<PathBuf>, Option<String>) {
        (self.turn_modified_files, self.last_tool_stdout)
    }
}

struct OutcomeContext<'a> {
    session_stats: &'a mut SessionStats,
    renderer: &'a mut AnsiRenderer,
    handle: &'a InlineHandle,
    harness_state: &'a mut crate::agent::runloop::unified::run_loop_context::HarnessTurnState,
    mcp_panel_state: &'a mut McpPanelState,
    vt_config: Option<&'a VTCodeConfig>,
    workspace_root: Option<&'a Path>,
}

struct SuccessPayload<'a> {
    output: &'a serde_json::Value,
    stdout: &'a Option<String>,
    modified_files: &'a [String],
    command_success: bool,
}

async fn handle_success_common(
    ctx: &mut OutcomeContext<'_>,
    name: &str,
    args_val: &serde_json::Value,
    payload: SuccessPayload<'_>,
    state: &mut OutcomeState,
) -> Result<()> {
    ctx.session_stats.record_tool(name);

    if let Some(tool_name) = name.strip_prefix("mcp_") {
        let tool_name = tool_name.trim_start_matches('_');
        let tool_name = tool_name.split("__").last().unwrap_or(tool_name);
        record_mcp_success_event(ctx.mcp_panel_state, tool_name, args_val);
    } else if is_task_tracker_tool(name) && ctx.renderer.supports_inline_ui() {
        let block_lines = task_tracker_block_lines(payload.output);
        if !block_lines.is_empty() {
            ctx.handle.update_task_panel_with_metadata(
                block_lines.clone(),
                crate::agent::runloop::tool_output::tracker_panel_metadata(payload.output),
            );
            apply_task_tracker_block(ctx.handle, ctx.harness_state, block_lines);
        }
    } else {
        render_tool_output_common(
            ctx.renderer,
            ctx.handle,
            name,
            args_val,
            payload.output,
            payload.command_success,
            ctx.vt_config,
            ctx.workspace_root,
        )
        .await?;
    }

    state.last_tool_stdout = if payload.command_success {
        payload.stdout.clone()
    } else {
        None
    };

    if !payload.modified_files.is_empty() {
        state.turn_modified_files.extend(collect_modified_files(payload.modified_files));
    }

    Ok(())
}

fn handle_non_success_common(
    ctx: &mut OutcomeContext<'_>,
    name: &str,
    args_val: &serde_json::Value,
    status: &ToolExecutionStatus,
) -> Result<()> {
    // PTY tools already rendered "• Ran ..." in the pre-execution inline block.
    // Skip duplicating the summary bullet here; the error/cancelled message
    // below is the only post-execution indicator needed.
    let is_pty = ctx.renderer.supports_inline_ui() && is_run_pty_tool(name, args_val);

    match status {
        ToolExecutionStatus::Failure { error } | ToolExecutionStatus::Timeout { error } => {
            let user_message = error.user_message();
            if ctx.renderer.supports_inline_ui() && is_command_output_call(name, args_val) {
                ctx.handle.record_tool_output(vec![
                    command_output_header(name, args_val, ctx.workspace_root),
                    format!(
                        "    {}: {}",
                        if matches!(status, ToolExecutionStatus::Timeout { .. }) {
                            "timed out"
                        } else {
                            "failed"
                        },
                        user_message
                    ),
                ]);
            }
            if !is_pty {
                render_non_success_summary(
                    ctx.renderer,
                    name,
                    args_val,
                    Some("error"),
                    ctx.workspace_root,
                    ToolDisplayStatus::Failure,
                )?;
            }
            render_error_common(
                ctx.renderer,
                name,
                &user_message,
                if matches!(status, ToolExecutionStatus::Timeout { .. }) {
                    "timed out"
                } else {
                    "failure"
                },
            )?;
        }
        ToolExecutionStatus::Cancelled => {
            if ctx.renderer.supports_inline_ui() && is_command_output_call(name, args_val) {
                ctx.handle.record_tool_output(vec![
                    command_output_header(name, args_val, ctx.workspace_root),
                    "    warning: tool execution cancelled".to_string(),
                ]);
            }
            if !is_pty {
                render_non_success_summary(
                    ctx.renderer,
                    name,
                    args_val,
                    Some("cancelled"),
                    ctx.workspace_root,
                    ToolDisplayStatus::Warning,
                )?;
            }
            ctx.renderer.line(MessageStyle::Info, "Tool execution cancelled")?;
        }
        ToolExecutionStatus::Success { .. } => {}
    }

    Ok(())
}

fn render_non_success_summary(
    renderer: &mut AnsiRenderer,
    name: &str,
    args_val: &serde_json::Value,
    stream_label: Option<&str>,
    workspace_root: Option<&Path>,
    status: ToolDisplayStatus,
) -> Result<()> {
    let summary_ctx = crate::agent::runloop::unified::tool_summary::ToolSummaryRenderContext { workspace_root };
    crate::agent::runloop::unified::tool_summary::render_expanded_tool_call_summary(
        renderer,
        name,
        args_val,
        stream_label,
        &summary_ctx,
        status.color(ColorPalette::default()),
    )
}

async fn process_outcome_common(
    ctx: &mut OutcomeContext<'_>,
    name: &str,
    args_val: &serde_json::Value,
    outcome: &ToolPipelineOutcome,
) -> Result<OutcomeState> {
    let mut state = OutcomeState::default();

    match &outcome.status {
        ToolExecutionStatus::Success {
            output, stdout, modified_files, command_success, ..
        } => {
            handle_success_common(
                ctx,
                name,
                args_val,
                SuccessPayload {
                    output,
                    stdout,
                    modified_files,
                    command_success: *command_success,
                },
                &mut state,
            )
            .await?;
        }
        _ => handle_non_success_common(ctx, name, args_val, &outcome.status)?,
    }

    Ok(state)
}

pub(crate) async fn handle_pipeline_output(
    ctx: &mut RunLoopContext<'_>,
    name: &str,
    args_val: &serde_json::Value,
    outcome: &ToolPipelineOutcome,
    vt_config: Option<&VTCodeConfig>,
) -> Result<(Vec<PathBuf>, Option<String>)> {
    // The registry owns the workspace used by the executor and the spooler.
    // Use it here even on the Copilot path, whose lightweight run-loop
    // context intentionally does not carry an auto-permission context.
    let workspace_root = Some(ctx.tool_registry.workspace_root().as_path());
    let mut output_ctx = OutcomeContext {
        session_stats: ctx.session_stats,
        renderer: ctx.renderer,
        handle: ctx.handle,
        harness_state: ctx.harness_state,
        mcp_panel_state: ctx.mcp_panel_state,
        vt_config,
        workspace_root,
    };
    let state = process_outcome_common(&mut output_ctx, name, args_val, outcome).await?;
    Ok(state.into_tuple())
}

// Adapter for TurnLoopContext (to avoid duplication when handling tool output in the turn loop)
pub(crate) async fn handle_pipeline_output_from_turn_ctx(
    ctx: &mut crate::agent::runloop::unified::turn::TurnLoopContext<'_>,
    name: &str,
    args_val: &serde_json::Value,
    outcome: &ToolPipelineOutcome,
    vt_config: Option<&VTCodeConfig>,
) -> Result<(Vec<PathBuf>, Option<String>)> {
    let mut run_ctx = ctx.as_run_loop_context();
    let (modified_files, last_stdout) =
        handle_pipeline_output(&mut run_ctx, name, args_val, outcome, vt_config).await?;

    if let ToolExecutionStatus::Success { output, modified_files, .. } = &outcome.status {
        let activity_paths =
            collect_instruction_activity_paths(ctx.config.workspace.as_path(), args_val, output, modified_files);
        if !activity_paths.is_empty() {
            ctx.context_manager.record_instruction_activity_paths(activity_paths);
        }
    }

    Ok((modified_files, last_stdout))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{IsTerminal, stdin};
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::sync::{RwLock, mpsc::unbounded_channel};
    use vtcode_core::acp::ToolPermissionCache;
    use vtcode_core::config::loader::VTCodeConfig;
    use vtcode_core::core::decision_tracker::DecisionTracker;
    use vtcode_core::core::trajectory::TrajectoryLogger;
    use vtcode_core::tools::ApprovalRecorder;
    use vtcode_core::tools::registry::ToolRegistry;
    use vtcode_core::tools::result_cache::{ToolCacheKey, ToolResultCache};
    use vtcode_core::ui::inline_theme_from_core_styles;
    use vtcode_core::ui::theme;
    use vtcode_ui::tui::app::{InlineCommand, InlineHandle, SessionOptions, spawn_session_with_options};

    fn build_harness_state() -> crate::agent::runloop::unified::run_loop_context::HarnessTurnState {
        crate::agent::runloop::unified::run_loop_context::HarnessTurnState::new(
            crate::agent::runloop::unified::run_loop_context::TurnRunId("test-run".to_string()),
            crate::agent::runloop::unified::run_loop_context::TurnId("test-turn".to_string()),
            4,
            60,
            0,
        )
    }

    fn dummy_handle() -> InlineHandle {
        InlineHandle::new_for_tests(unbounded_channel().0)
    }

    #[test]
    fn successful_task_tracker_replacement_contains_only_compact_tree_rows() {
        // Successful updates replace the prior tracker block as one compact
        // tree. Tool-call arguments are operational detail, not task-panel or
        // transcript content.
        let (sender, mut receiver) = unbounded_channel();
        let handle = InlineHandle::new_for_tests(sender);
        let mut harness_state = build_harness_state();
        let first = serde_json::json!({
            "status": "updated",
            "checklist": {
                "items": [
                    { "index_path": "1", "level": 0, "description": "Release", "status": "in_progress" },
                    { "index_path": "1.1", "level": 1, "description": "Update version", "status": "completed" },
                    { "index_path": "1.2", "level": 1, "description": "Run checks", "status": "in_progress" }
                ]
            }
        });
        let second = serde_json::json!({
            "status": "updated",
            "checklist": {
                "items": [
                    { "index_path": "1", "level": 0, "description": "Release", "status": "completed" },
                    { "index_path": "1.1", "level": 1, "description": "Update version", "status": "completed" },
                    { "index_path": "1.2", "level": 1, "description": "Run checks", "status": "completed" }
                ]
            }
        });

        apply_task_tracker_block(&handle, &mut harness_state, task_tracker_block_lines(&first));
        apply_task_tracker_block(&handle, &mut harness_state, task_tracker_block_lines(&second));

        let replacement = std::iter::from_fn(|| receiver.try_recv().ok()).find_map(|command| match command {
            InlineCommand::ReplaceLast { count, lines, .. } => Some((count, lines)),
            _ => None,
        });
        let (count, rows) = replacement.expect("second tracker update should replace the previous compact tree");
        let rows = rows
            .into_iter()
            .map(|row| row.into_iter().map(|segment| segment.text).collect::<String>())
            .collect::<Vec<_>>();

        assert_eq!(count, 4);
        assert_eq!(
            rows,
            vec![
                "• Task tracker",
                "  └ Release",
                "    [x] Update version",
                "    [x] Run checks",
            ]
        );
    }

    // Use Tokio runtime for async test blocks
    #[tokio::test]
    async fn test_renderer_records_tool_and_collects_modified_files() {
        // Setup a stdout renderer
        let mut renderer = AnsiRenderer::stdout();

        // Prepare session stats and mcp state
        let mut stats = SessionStats::default();
        let mut mcp = McpPanelState::default();

        // Create an outcome that indicates write to /tmp/foo.txt
        let output_json = serde_json::json!({"result":"ok"});
        let outcome = ToolPipelineOutcome::from_status(ToolExecutionStatus::Success {
            output: output_json.clone(),
            stdout: None,
            modified_files: vec!["/tmp/foo.txt".to_string()],
            command_success: true,
        });

        // Invoke the shared outcome processor via a minimal output context.
        let handle = dummy_handle();
        let mut harness_state = build_harness_state();
        let mut output_ctx = OutcomeContext {
            workspace_root: None,
            session_stats: &mut stats,
            renderer: &mut renderer,
            handle: &handle,
            harness_state: &mut harness_state,
            mcp_panel_state: &mut mcp,
            vt_config: None::<&VTCodeConfig>,
        };
        let (mod_files, _last_stdout) =
            process_outcome_common(&mut output_ctx, "write_file", &serde_json::json!({}), &outcome)
                .await
                .expect("render should succeed")
                .into_tuple();

        // Confirm the function recorded the tool call
        let recorded = stats.sorted_tools();
        assert!(recorded.contains(&"write_file".to_string()));

        // Confirm the modified files list contains our path
        assert_eq!(mod_files, vec![PathBuf::from("/tmp/foo.txt")]);
    }

    #[test]
    fn tool_call_visual_status_colors_success_failure_and_warning() {
        let palette = ColorPalette::default();
        assert_eq!(ToolDisplayStatus::Success.color(palette), palette.success);
        assert_eq!(ToolDisplayStatus::Failure.color(palette), palette.error);
        assert_eq!(ToolDisplayStatus::Warning.color(palette), palette.warning);

        assert!(matches!(
            ToolDisplayStatus::from_command_output(&serde_json::json!({}), true),
            ToolDisplayStatus::Success
        ));
        assert!(matches!(
            ToolDisplayStatus::from_command_output(&serde_json::json!({}), false),
            ToolDisplayStatus::Failure
        ));
        assert!(matches!(
            ToolDisplayStatus::from_command_output(&serde_json::json!({"warning": "no results"}), true),
            ToolDisplayStatus::Warning
        ));
        assert!(matches!(
            ToolDisplayStatus::from_command_output(&serde_json::json!({"warning": null}), true),
            ToolDisplayStatus::Success
        ));

        assert!(
            compact_run_completion_line(&serde_json::json!({"exit_code": 0}), ToolDisplayStatus::Success).is_some()
        );
        assert!(
            compact_run_completion_line(&serde_json::json!({"warning": "no results"}), ToolDisplayStatus::Warning)
                .is_some()
        );
        assert!(compact_run_completion_line(&serde_json::json!({}), ToolDisplayStatus::Success).is_none());
    }

    #[test]
    fn ordered_stream_texts_deduplicates_merged_output_aliases() {
        let output = serde_json::json!({
            "output": "stdout line\nstderr line",
            "stdout": "stdout line",
            "stderr": "stderr line"
        });

        assert_eq!(ordered_stream_texts(&output), vec!["stdout line\nstderr line"]);
    }

    #[test]
    fn ordered_stream_texts_preserves_distinct_pipe_streams() {
        let output = serde_json::json!({
            "output": "merged line",
            "stdout": "stdout line",
            "stderr": "stderr line"
        });

        assert_eq!(ordered_stream_texts(&output), vec!["merged line", "stdout line", "stderr line"]);
    }

    #[test]
    fn canonical_pipe_streams_keep_merged_output_once() {
        let output = serde_json::json!({
            "output": "stdout line\nstderr line",
            "stdout": "stdout line",
            "stderr": "stderr line"
        });

        let streams = canonical_pipe_streams(&output);
        assert_eq!(streams.len(), 1);
        assert_eq!(streams[0].label, None);
        assert_eq!(streams[0].text, "stdout line\nstderr line");
    }

    #[test]
    fn canonical_pipe_streams_label_separate_streams() {
        let output = serde_json::json!({
            "stdout": "stdout line",
            "stderr": "stderr line"
        });

        let streams = canonical_pipe_streams(&output);
        assert_eq!(
            streams.iter().map(|stream| (stream.label, stream.text)).collect::<Vec<_>>(),
            vec![(Some("stdout"), "stdout line"), (Some("stderr"), "stderr line")]
        );
    }

    #[test]
    fn canonical_pipe_streams_preserve_full_named_alias() {
        let output = serde_json::json!({
            "output": "stdout line",
            "stdout": "stdout line\nsecond stdout line",
            "stderr": "stderr line"
        });

        let streams = canonical_pipe_streams(&output);
        assert_eq!(
            streams.iter().map(|stream| (stream.label, stream.text)).collect::<Vec<_>>(),
            vec![
                (Some("stdout"), "stdout line\nsecond stdout line"),
                (Some("stderr"), "stderr line")
            ]
        );
    }

    #[test]
    fn normalize_terminal_output_lines_handles_ansi_rewrites_and_blanks() {
        let capture = "stale\n\x1b[2J\x1b[H\x1b[31mred\x1b[0m\rfinal\n\nlast\n";

        assert_eq!(normalize_terminal_output_lines(capture), vec!["final", "", "last"]);
        assert_eq!(normalize_terminal_output_lines("abc\x08d\n"), vec!["abd"]);
    }

    #[test]
    fn build_pipe_command_output_lines_labels_stderr_once() {
        let output = serde_json::json!({
            "stdout": "normal output",
            "stderr": "diagnostic output",
            "exit_code": 1
        });

        assert_eq!(
            build_pipe_command_output_lines(
                tools::EXECUTE_CODE,
                &serde_json::json!({"command": "printf test"}),
                &output,
                None,
                ToolDisplayStatus::Failure,
            ),
            vec![
                "• Ran printf test",
                "  stdout:",
                "    normal output",
                "  stderr:",
                "    diagnostic output",
                "    ✗ run error, exit code: 1",
            ]
        );
    }

    #[test]
    fn build_merged_command_output_lines_keeps_complete_capture_and_status_once() {
        let output = serde_json::json!({
            "exit_code": 2,
            "critical_note": "output was retained in the current session"
        });
        let capture = "stdout line\nstderr line\n";

        let lines = build_merged_command_output_lines(
            tools::RUN_PTY_CMD,
            &serde_json::json!({"command": "long command"}),
            capture,
            None,
            &output,
            ToolDisplayStatus::Failure,
        );

        assert_eq!(lines[0], "• Ran long command");
        assert!(lines.contains(&"  └ stdout line".to_string()));
        assert!(lines.contains(&"    stderr line".to_string()));
        assert!(lines.contains(&"    output was retained in the current session".to_string()));
        assert_eq!(lines.iter().filter(|line| line.contains("stderr line")).count(), 1);
        assert_eq!(lines.iter().filter(|line| line.contains("exit code: 2")).count(), 1);
    }

    #[tokio::test]
    async fn pty_capture_reads_complete_workspace_spool() {
        let workspace = TempDir::new().expect("workspace temp dir");
        let spool_path = workspace.path().join(".vtcode/context/tool_outputs/pty.txt");
        tokio::fs::create_dir_all(spool_path.parent().expect("spool parent"))
            .await
            .expect("create spool parent");
        tokio::fs::write(&spool_path, "first complete line\nsecond complete line\n")
            .await
            .expect("write spool");

        let output = serde_json::json!({
            "spool_path": ".vtcode/context/tool_outputs/pty.txt",
            "output": "first preview line"
        });

        assert_eq!(
            load_complete_output(&output, Some(workspace.path())).await.as_deref(),
            Some("first complete line\nsecond complete line\n")
        );
    }

    #[tokio::test]
    async fn pty_capture_rejects_spool_outside_workspace() {
        let workspace = TempDir::new().expect("workspace temp dir");
        let outside = TempDir::new().expect("outside temp dir");
        let spool_path = outside.path().join("pty.txt");
        tokio::fs::write(&spool_path, "secret outside workspace")
            .await
            .expect("write outside spool");

        let output = serde_json::json!({ "spool_path": spool_path });

        assert!(load_complete_output(&output, Some(workspace.path())).await.is_none());
    }

    #[tokio::test]
    async fn pty_capture_rejects_malformed_spool_metadata_without_inline_fallback() {
        let workspace = TempDir::new().expect("workspace temp dir");
        let output = serde_json::json!({
            "spool_path": null,
            "output": "untrusted inline fallback"
        });

        assert!(load_complete_output(&output, Some(workspace.path())).await.is_none());
    }

    #[tokio::test]
    async fn test_renderer_records_mcp_event_for_mcp_tool() {
        let mut renderer = AnsiRenderer::stdout();

        // Note: tests involving `apply_turn_outcome` live in `turn/turn_loop.rs` and can be added there
        let mut stats = SessionStats::default();
        let mut mcp = McpPanelState::new(32, true); // enabled

        let output_json = serde_json::json!({"exit_code":0});
        let outcome = ToolPipelineOutcome::from_status(ToolExecutionStatus::Success {
            output: output_json.clone(),
            stdout: Some("ok".to_string()),
            modified_files: vec![],
            command_success: true,
        });

        let handle = dummy_handle();
        let mut harness_state = build_harness_state();
        let mut output_ctx = OutcomeContext {
            workspace_root: None,
            session_stats: &mut stats,
            renderer: &mut renderer,
            handle: &handle,
            harness_state: &mut harness_state,
            mcp_panel_state: &mut mcp,
            vt_config: None::<&VTCodeConfig>,
        };
        let (_mod_files, _last_stdout) =
            process_outcome_common(&mut output_ctx, "mcp_example", &serde_json::json!({}), &outcome)
                .await
                .expect("render should succeed")
                .into_tuple();

        // Ensure mcp panel recorded an event
        assert!(mcp.event_count() > 0);
    }

    #[tokio::test]
    async fn spooled_exec_output_keeps_transcript_at_reference_only() {
        let mut renderer = AnsiRenderer::stdout();
        let mut stats = SessionStats::default();
        let mut mcp = McpPanelState::default();
        let handle = dummy_handle();
        let mut harness_state = build_harness_state();

        transcript::clear();

        let outcome = ToolPipelineOutcome::from_status(ToolExecutionStatus::Success {
            output: serde_json::json!({
                "output": "preview text that should stay out of transcript persistence",
                "spool_path": ".vtcode/context/tool_outputs/exec_command_1.txt",
                "exit_code": 0,
                "is_exited": true
            }),
            stdout: Some("preview text that should stay out of transcript persistence".to_string()),
            modified_files: vec![],
            command_success: true,
        });

        let mut output_ctx = OutcomeContext {
            workspace_root: None,
            session_stats: &mut stats,
            renderer: &mut renderer,
            handle: &handle,
            harness_state: &mut harness_state,
            mcp_panel_state: &mut mcp,
            vt_config: None::<&VTCodeConfig>,
        };

        process_outcome_common(
            &mut output_ctx,
            tools::UNIFIED_EXEC,
            &serde_json::json!({
                "action": "run",
                "command": "cargo check -p vtcode-core"
            }),
            &outcome,
        )
        .await
        .expect("render should succeed");

        let transcript_lines = transcript::snapshot();
        let transcript_text = transcript_lines.join("\n");
        let stripped_text = vtcode_core::utils::ansi_parser::strip_ansi(&transcript_text);
        assert!(stripped_text.contains("Large output was spooled to"), "Transcript: {stripped_text:?}");
        assert!(!stripped_text.contains("preview text that should stay out of transcript persistence"));

        transcript::clear();
    }

    #[tokio::test]
    async fn inline_tool_output_viewer_retains_complete_spooled_capture() {
        let workspace = TempDir::new().expect("workspace temp dir");
        let spool_path = workspace.path().join(".vtcode/context/tool_outputs/exec_command_1.txt");
        tokio::fs::create_dir_all(spool_path.parent().expect("spool parent"))
            .await
            .expect("create spool parent");
        tokio::fs::write(&spool_path, "first complete line\nsecond complete line\n")
            .await
            .expect("write spool");

        let (sender, mut receiver) = unbounded_channel();
        let handle = InlineHandle::new_for_tests(sender);
        let mut renderer = AnsiRenderer::with_inline_ui(handle.clone(), Default::default());
        let mut stats = SessionStats::default();
        let mut mcp = McpPanelState::default();
        let mut harness_state = build_harness_state();
        transcript::clear();
        let outcome = ToolPipelineOutcome::from_status(ToolExecutionStatus::Success {
            output: serde_json::json!({
                "output": "preview line",
                "spool_path": ".vtcode/context/tool_outputs/exec_command_1.txt",
                "exit_code": 0,
                "is_exited": true
            }),
            stdout: Some("preview line".to_string()),
            modified_files: vec![],
            command_success: true,
        });
        let mut output_ctx = OutcomeContext {
            workspace_root: Some(workspace.path()),
            session_stats: &mut stats,
            renderer: &mut renderer,
            handle: &handle,
            harness_state: &mut harness_state,
            mcp_panel_state: &mut mcp,
            vt_config: None::<&VTCodeConfig>,
        };

        process_outcome_common(
            &mut output_ctx,
            tools::RUN_PTY_CMD,
            &serde_json::json!({"command": "cargo check"}),
            &outcome,
        )
        .await
        .expect("render should succeed");

        let mut recorded = None;
        while let Ok(command) = receiver.try_recv() {
            if let InlineCommand::RecordToolOutput { lines } = command {
                recorded = Some(lines);
            }
        }
        let lines = recorded.expect("the complete output should be recorded for the viewer");
        assert_eq!(lines[0], "• Ran cargo check");
        assert!(lines.iter().any(|line| line == "  └ first complete line"));
        assert!(lines.iter().any(|line| line == "    second complete line"));
        assert!(!lines.iter().any(|line| line.contains("preview line")));

        let transcript_text = transcript::snapshot().join("\n");
        assert!(!transcript_text.contains("first complete line"));
        assert!(!transcript_text.contains("second complete line"));
        transcript::clear();
    }

    #[tokio::test]
    async fn test_handle_pipeline_output_collects_modified_files_and_records_stats() {
        if !stdin().is_terminal() {
            eprintln!("Skipping TUI-dependent test in non-interactive environment");
            return;
        }

        let tmp = TempDir::new().unwrap();
        let workspace = tmp.path().to_path_buf();

        let mut registry = ToolRegistry::new(workspace.clone()).await;
        let permission_cache_arc = Arc::new(RwLock::new(ToolPermissionCache::new()));
        let permissions_state = Arc::new(RwLock::new(vtcode_core::config::PermissionsConfig::default()));

        let mut session = spawn_session_with_options(
            inline_theme_from_core_styles(&theme::active_styles()),
            SessionOptions {
                inline_rows: 10,
                workspace_root: Some(workspace.clone()),
                ..SessionOptions::default()
            },
        )
        .unwrap();
        let handle = session.clone_inline_handle();
        let mut renderer = AnsiRenderer::with_inline_ui(handle.clone(), Default::default());

        let cache = Arc::new(RwLock::new(ToolResultCache::new(8)));
        let key = ToolCacheKey::new("read_file", "{}", "/tmp/foo.txt");
        {
            let mut c = cache.write().await;
            c.insert_arc(key.clone(), Arc::new("{}".to_string()));
            assert!(c.get(&key).is_some());
        }

        let decision_ledger = Arc::new(RwLock::new(DecisionTracker::new()));
        let mut session_stats = SessionStats::default();
        let mut plan_session =
            crate::agent::runloop::unified::planning_workflow_state::PlanningWorkflowSessionState::default();
        let mut mcp_panel = McpPanelState::new(10, true);
        let approval_recorder = ApprovalRecorder::new(workspace.clone());
        let traj = TrajectoryLogger::new(&workspace);
        let tools = Arc::new(RwLock::new(Vec::new()));

        let mut harness_state = build_harness_state();
        let mut ctx = RunLoopContext::new(
            &mut renderer,
            &handle,
            &mut registry,
            &tools,
            &cache,
            &permission_cache_arc,
            &permissions_state,
            &decision_ledger,
            &mut session_stats,
            &mut plan_session,
            &mut mcp_panel,
            &approval_recorder,
            &mut session,
            None,
            &traj,
            &mut harness_state,
            None,
        );

        let outcome = ToolPipelineOutcome::from_status(ToolExecutionStatus::Success {
            output: serde_json::json!({"ok": true}),
            stdout: None,
            modified_files: vec!["/tmp/foo.txt".to_string()],
            command_success: true,
        });

        let (mod_files, _last_stdout) =
            handle_pipeline_output(&mut ctx, "read_file", &serde_json::json!({}), &outcome, None::<&VTCodeConfig>)
                .await
                .expect("handle should succeed");

        assert_eq!(mod_files, vec![PathBuf::from("/tmp/foo.txt")]);

        // Cache invalidation is handled in execution side-effects, not output rendering.
        {
            let c = cache.write().await;
            assert!(c.get(&key).is_some());
        }

        // Ensure session stats were updated
        let rec = session_stats.sorted_tools();
        assert!(rec.contains(&"read_file".to_string()));
    }

    #[tokio::test]
    async fn task_tracker_updates_replace_previous_inline_block() {
        transcript::clear();

        let (sender, mut receiver) = unbounded_channel();
        let handle = InlineHandle::new_for_tests(sender);
        let mut renderer = AnsiRenderer::with_inline_ui(handle.clone(), Default::default());
        let mut stats = SessionStats::default();
        let mut mcp = McpPanelState::default();
        let mut harness_state = build_harness_state();

        let first = ToolPipelineOutcome::from_status(ToolExecutionStatus::Success {
            output: serde_json::json!({
                "status": "updated",
                "view": {
                    "title": "Respond to user greeting and assess next steps",
                    "lines": [
                        {"display": "├ ✔ Greet user and summarize current workspace state"},
                        {"display": "├ > Ask what task they'd like to tackle"},
                        {"display": "└ • Offer to provide workspace tour if needed"}
                    ]
                },
                "checklist": {
                    "title": "Respond to user greeting and assess next steps",
                    "total": 3,
                    "completed": 1,
                    "in_progress": 1,
                    "pending": 1,
                    "blocked": 0,
                    "progress_percent": 33,
                    "items": [
                        {"index": 1, "description": "Greet user and summarize current workspace state", "status": "completed"},
                        {"index": 2, "description": "Ask what task they'd like to tackle", "status": "in_progress"},
                        {"index": 3, "description": "Offer to provide workspace tour if needed", "status": "pending"}
                    ]
                },
                "message": "Item 2 status changed: pending → in_progress"
            }),
            stdout: None,
            modified_files: vec![],
            command_success: true,
        });
        let second = ToolPipelineOutcome::from_status(ToolExecutionStatus::Success {
            output: serde_json::json!({
                "status": "updated",
                "view": {
                    "title": "Respond to user greeting and assess next steps",
                    "lines": [
                        {"display": "├ ✔ Greet user and summarize current workspace state"},
                        {"display": "├ ✔ Ask what task they'd like to tackle"},
                        {"display": "└ • Offer to provide workspace tour if needed"}
                    ]
                },
                "checklist": {
                    "title": "Respond to user greeting and assess next steps",
                    "total": 3,
                    "completed": 2,
                    "in_progress": 0,
                    "pending": 1,
                    "blocked": 0,
                    "progress_percent": 67,
                    "items": [
                        {"index": 1, "description": "Greet user and summarize current workspace state", "status": "completed"},
                        {"index": 2, "description": "Ask what task they'd like to tackle", "status": "completed"},
                        {"index": 3, "description": "Offer to provide workspace tour if needed", "status": "pending"}
                    ]
                },
                "message": "Item 2 status changed: in_progress → completed"
            }),
            stdout: None,
            modified_files: vec![],
            command_success: true,
        });

        let args = serde_json::json!({"action": "update", "index": 2, "status": "in_progress"});
        let mut output_ctx = OutcomeContext {
            workspace_root: None,
            session_stats: &mut stats,
            renderer: &mut renderer,
            handle: &handle,
            harness_state: &mut harness_state,
            mcp_panel_state: &mut mcp,
            vt_config: None::<&VTCodeConfig>,
        };

        process_outcome_common(&mut output_ctx, tools::TASK_TRACKER, &args, &first)
            .await
            .expect("first tracker render should succeed");

        let args = serde_json::json!({"action": "update", "index": 2, "status": "completed"});
        process_outcome_common(&mut output_ctx, tools::TASK_TRACKER, &args, &second)
            .await
            .expect("second tracker render should succeed");

        let mut saw_task_panel_update = false;
        while let Ok(command) = receiver.try_recv() {
            if matches!(command, InlineCommand::ShowTransient { .. }) {
                saw_task_panel_update = true;
            }
        }

        assert!(saw_task_panel_update, "expected tracker updates to refresh the dedicated task panel");
    }

    #[tokio::test]
    async fn test_handle_pipeline_output_mcp_events() {
        if !stdin().is_terminal() {
            eprintln!("Skipping TUI-dependent test in non-interactive environment");
            return;
        }

        let tmp = TempDir::new().unwrap();
        let workspace = tmp.path().to_path_buf();

        let mut registry = ToolRegistry::new(workspace.clone()).await;
        let permission_cache_arc = Arc::new(RwLock::new(ToolPermissionCache::new()));
        let permissions_state = Arc::new(RwLock::new(vtcode_core::config::PermissionsConfig::default()));

        let mut session = spawn_session_with_options(
            inline_theme_from_core_styles(&theme::active_styles()),
            SessionOptions {
                inline_rows: 10,
                workspace_root: Some(workspace.clone()),
                ..SessionOptions::default()
            },
        )
        .unwrap();
        let handle = session.clone_inline_handle();
        let mut renderer = AnsiRenderer::with_inline_ui(handle.clone(), Default::default());

        let cache = Arc::new(RwLock::new(ToolResultCache::new(8)));
        let decision_ledger = Arc::new(RwLock::new(DecisionTracker::new()));
        let mut session_stats = SessionStats::default();
        let mut plan_session =
            crate::agent::runloop::unified::planning_workflow_state::PlanningWorkflowSessionState::default();
        let mut mcp_panel = McpPanelState::new(10, true);
        let approval_recorder = ApprovalRecorder::new(workspace.clone());
        let traj = TrajectoryLogger::new(&workspace);
        let tools = Arc::new(RwLock::new(Vec::new()));

        let mut harness_state = build_harness_state();
        let mut ctx = RunLoopContext::new(
            &mut renderer,
            &handle,
            &mut registry,
            &tools,
            &cache,
            &permission_cache_arc,
            &permissions_state,
            &decision_ledger,
            &mut session_stats,
            &mut plan_session,
            &mut mcp_panel,
            &approval_recorder,
            &mut session,
            None,
            &traj,
            &mut harness_state,
            None,
        );

        let outcome = ToolPipelineOutcome::from_status(ToolExecutionStatus::Success {
            output: serde_json::json!({"exit_code": 0}),
            stdout: Some("ok".to_string()),
            modified_files: vec![],
            command_success: true,
        });

        let (_mod_files, _last_stdout) =
            handle_pipeline_output(&mut ctx, "mcp_example", &serde_json::json!({}), &outcome, None::<&VTCodeConfig>)
                .await
                .expect("handle should succeed");

        assert!(ctx.mcp_panel_state.event_count() > 0);
    }
}
