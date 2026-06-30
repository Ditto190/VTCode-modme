#![allow(unused_imports)]

#[allow(unused_imports)]
use super::*;

pub(super) fn ast_grep_command(
    ast_grep: &Path,
    workspace_root: &Path,
    subcommand: &str,
) -> Command {
    let mut command = Command::new(ast_grep);
    command.current_dir(workspace_root).arg(subcommand);
    command
}

pub(super) async fn run_ast_grep_command(
    command: &mut Command,
    context: &str,
) -> Result<std::process::Output> {
    command.output().await.with_context(|| context.to_string())
}

/// Apply the common ast-grep CLI flags shared across query, rewrite, count,
/// and apply workflows: `--lang`, `--selector`, `--strictness`, context/globs,
/// `--follow`, and `--no-ignore`.
pub(super) fn apply_common_run_flags(
    command: &mut Command,
    request: &StructuralSearchRequest,
    globs: &[String],
) {
    if let Some(lang) = request
        .lang
        .as_deref()
        .filter(|lang| !lang.trim().is_empty())
    {
        command.arg("--lang").arg(lang);
    }
    if let Some(selector) = request
        .selector
        .as_deref()
        .filter(|selector| !selector.trim().is_empty())
    {
        command.arg("--selector").arg(selector);
    }
    if let Some(strictness) = &request.strictness {
        command.arg("--strictness").arg(strictness.as_str());
    }
    apply_context_and_globs(
        command,
        request.context_lines,
        request.effective_before_lines(),
        request.effective_after_lines(),
        globs,
    );
    if request.effective_follow() {
        command.arg("--follow");
    }
    if let Some(no_ignore) = request.effective_no_ignore() {
        for value in no_ignore {
            command.arg("--no-ignore").arg(value.trim());
        }
    }
}

pub(super) fn apply_context_and_globs(
    command: &mut Command,
    context_lines: Option<usize>,
    before_lines: Option<usize>,
    after_lines: Option<usize>,
    globs: &[String],
) {
    if let Some(before) = before_lines {
        command.arg("--before").arg(before.to_string());
    }
    if let Some(after) = after_lines {
        command.arg("--after").arg(after.to_string());
    }
    // Symmetric context only when before/after are not set (validated upstream).
    if before_lines.is_none()
        && after_lines.is_none()
        && let Some(context_lines) = context_lines
    {
        command.arg("--context").arg(context_lines.to_string());
    }
    for glob in globs {
        command.arg("--globs").arg(glob);
    }
}

pub(super) fn build_debug_query_result(
    request: &StructuralSearchRequest,
    display_path: &str,
    debug_query: &DebugQueryFormat,
    stdout: &[u8],
) -> Value {
    let mut result = json!({
        "backend": "ast-grep",
        "path": display_path,
        "lang": request.lang,
        "debug_query": debug_query.as_str(),
        "debug_query_output": truncate_auxiliary_output(String::from_utf8_lossy(stdout).trim()),
        "matches": [],
        "truncated": false,
    });
    if let Some(pattern) = request.pattern() {
        result["pattern"] = json!(pattern);
    }
    if let Some(kind) = request.kind() {
        result["kind"] = json!(kind);
    }
    result
}
