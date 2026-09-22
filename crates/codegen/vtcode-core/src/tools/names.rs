// Re-export from shared utils to break the tool_policy <-> tools cycle.
pub use crate::utils::tool_name_parsing::canonical_tool_name;

/// Returns true when a bare shell token names the `apply_patch` tool
/// (including the `applypatch` alias), with or without a path prefix.
fn is_apply_patch_tool_name(name: &str) -> bool {
    let base = name.rsplit('/').next().unwrap_or(name);
    matches!(base, "apply_patch" | "applypatch")
}

/// Returns true when a full shell command line tries to run the
/// `apply_patch` tool as a shell binary (e.g. `apply_patch`, or
/// `/usr/bin/applypatch --help`). Only the first token is considered, so
/// `sudo apply_patch` is not a collision.
pub fn is_apply_patch_shell_collision_command(command: &str) -> bool {
    if let Ok(parts) = shell_words::split(command)
        && let Some(first) = parts.into_iter().next()
        && !first.trim().is_empty()
    {
        return is_apply_patch_tool_name(&first);
    }
    // Fallback for unbalanced quoting, where `shell_words` fails: whitespace
    // split with surrounding-quote trimming still identifies the tool name.
    let first_token = command.split_whitespace().next().unwrap_or("").trim();
    let base = first_token.rsplit('/').next().unwrap_or(first_token);
    let base_trimmed = base.trim_matches(|c| c == '\'' || c == '"' || c == '`');
    matches!(base_trimmed, "apply_patch" | "applypatch")
}

#[test]
fn test_canonical_tool_name_passes_through() {
    // With registration-based aliases, this function now just passes through
    // Alias resolution happens earlier in the inventory layer
    assert_eq!(canonical_tool_name("list_files"), "list_files");

    assert_eq!(canonical_tool_name("unknown_tool"), "unknown_tool");

    assert_eq!(canonical_tool_name("container.exec"), "container.exec");
}

#[test]
fn test_is_apply_patch_shell_collision_command() {
    assert!(is_apply_patch_shell_collision_command("apply_patch"));
    assert!(is_apply_patch_shell_collision_command("applypatch"));
    assert!(is_apply_patch_shell_collision_command("apply_patch --help"));
    assert!(is_apply_patch_shell_collision_command("/usr/bin/apply_patch"));
    assert!(is_apply_patch_shell_collision_command("./applypatch foo"));
    assert!(is_apply_patch_shell_collision_command("'apply_patch'"));
    assert!(is_apply_patch_shell_collision_command("apply_patch \"unclosed"));
    assert!(!is_apply_patch_shell_collision_command("sudo apply_patch"));
    assert!(!is_apply_patch_shell_collision_command("pip install foo"));
    assert!(!is_apply_patch_shell_collision_command(""));
    assert!(!is_apply_patch_shell_collision_command("APPLY_PATCH"));
}
