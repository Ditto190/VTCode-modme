# Audit: Raw println!/print! calls that could leak into the TUI

## Summary

Raw `println!`/`print!` calls bypass crossterm's terminal management, causing:
- Corrupted display (text appears over TUI rendering)
- Blocked event loop (when paired with `io::stdin().read_line()`)
- Screen flashing or black screens

## Risk Categories

### HIGH RISK - Active in TUI, no guard

| File | Line | Issue |
|------|------|-------|
| `src/agent/runloop/mcp_elicitation.rs` | 97 | `print!("Response> ")` + `io::stdin().read_line()` during MCP elicitation. No `is_tui_mode()` check. Would corrupt TUI and block event loop if an MCP server requests elicitation while TUI is active. |

### MEDIUM RISK - Latent, guarded by existing code paths

| File | Lines | Status |
|------|-------|--------|
| `src/startup/workspace_trust.rs` | 72, 79, 86, 142, 220-303 | `ensure_full_auto_workspace_trust` status messages and `render_prompt`/`read_user_selection`. Currently safe because: (1) `prompt_capable()` returns false in TUI mode, preventing the interactive prompt; (2) `is_workspace_trusted()` check in `support.rs` prevents reaching the `NonInteractive` branch from TUI. **Latent risk**: if a new caller invokes `ensure_full_auto_workspace_trust` without the pre-check, the `println!` calls would corrupt the TUI. |

### SAFE - Properly guarded or CLI-only

| File | Lines | Guard |
|------|-------|-------|
| `src/agent/runloop/git.rs` | 327-328 | `is_tui_mode()` check skips interactive prompt in TUI |
| `src/agent/runloop/unified/postamble.rs` | 41-122 | Called after `restore_tui()` (session_loop_runner:1300) |
| `src/codex_app_server/runtime.rs` | 266, 439-492, 709-719, 930 | Codex app server has its own TUI (not crossterm-based) |
| `src/cli/` | various | CLI commands, not TUI context |
| `src/main.rs`, `src/main_helpers/` | various | Startup/shutdown, not TUI |
| `src/process_hardening.rs` | various | Error messages via `eprintln!`, not TUI |
| `src/startup/first_run_prompts/` | various | First-run setup, not TUI |
| `src/startup/workspace_trust.rs:159-176` | `prompt_capable()` | Returns false in TUI mode, preventing interactive prompt |

## Recommendations

### 1. Fix MCP elicitation handler (HIGH)

Add TUI mode guard to `InteractiveMcpElicitationHandler`:

```rust
// src/agent/runloop/mcp_elicitation.rs
if vtcode_core::ui::is_tui_mode() {
    // In TUI mode, auto-decline elicitation to avoid corrupting display
    tracing::info!("MCP elicitation declined in TUI mode");
    return Ok(McpElicitationResponse {
        action: ElicitationAction::Decline,
        content: None,
        meta: None,
    });
}
```

### 2. Guard `ensure_full_auto_workspace_trust` status messages (MEDIUM)

Add TUI mode check to the `println!` calls in `ensure_full_auto_workspace_trust`:

```rust
// src/startup/workspace_trust.rs
fn tui_safe_println(msg: &str) {
    if vtcode_core::ui::is_tui_mode() {
        tracing::info!("{}", msg);
    } else {
        println!("{}", msg);
    }
}
```

### 3. Add lint rule (PREVENTIVE)

Consider adding an `ast-grep` rule or CI check to flag `println!`/`print!` in `src/` outside of:
- `#[cfg(test)]` modules
- CLI command handlers (`src/cli/`)
- Files with explicit TUI mode guards

## Verification

After fixes, verify with:
```bash
# Find all non-test println/print calls in src/
grep -rn "println!\|print!(" --include="*.rs" src/ | grep -v "#\[test\]" | grep -v "// "
```

Any remaining hits outside `src/cli/` should have an `is_tui_mode()` guard or be provably TUI-safe.
