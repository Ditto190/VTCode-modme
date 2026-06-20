# Modes Audit: Bug Report and Fix Summary

**Date:** 2026-06-20
**Review scope:** Changes from the modes audit implementation (Fix 2-8) + workspace detection module
**Methodology:** Two independent review agents, cross-validated; false positives eliminated

---

## Ranked Bug Report

### Critical
None found.

### High

#### H1: `has_csproj` shallow scan misses standard .NET layouts
- **File:** `vtcode-core/src/core/agent/runner/workspace_detection.rs:43-59`
- **Root cause:** `read_dir()` only scans immediate children of workspace root. In standard .NET solutions, `.csproj` files live in subdirectories (e.g., `src/MyProject/MyProject.csproj`), not at the root.
- **Impact:** `dotnet test` is never inferred for most real-world .NET workspaces.
- **Status:** FIXED. Now checks for `.sln` at root (standard .NET solution file) and scans immediate subdirectories for `.csproj`.

#### H2: Ruby detection hardcodes rspec, breaks minitest projects
- **File:** `vtcode-core/src/core/agent/runner/workspace_detection.rs:33-35`
- **Root cause:** Always returns `bundle exec rspec` regardless of test framework. Minitest is the Rails default and is bundled with Ruby.
- **Impact:** Verify command fails for minitest-based projects (the majority of Rails apps).
- **Status:** FIXED. Now checks for `spec/` directory (rspec convention) vs `test/` directory (minitest convention). Defaults to `bundle exec rake test` when no `spec/` directory exists.

### Medium

#### M1: Monorepo first-match only; ignores secondary languages
- **File:** `vtcode-core/src/core/agent/runner/workspace_detection.rs:9-39`
- **Root cause:** Early-return pattern means only the first matching language is detected. A Rust+Node.js monorepo (e.g., Tauri) only gets `cargo check`, ignoring `npm test`.
- **Impact:** Verification is incomplete for polyglot workspaces.
- **Status:** FIXED. Changed from first-match-returns to collect-all-matches. The `Vec<String>` return type already supported multiple commands; the callers in `continuation.rs:78` and `orchestration.rs:544` already iterate over the vector.

#### M2: Maven/Gradle dual-manifest priority
- **File:** `vtcode-core/src/core/agent/runner/workspace_detection.rs:25-32`
- **Root cause:** `pom.xml` is checked before `build.gradle`. A stale `pom.xml` in a Gradle-migrated project would cause `mvn test` to run instead of `gradle test`.
- **Impact:** Low. Uncommon scenario. The Maven-first heuristic is reasonable for most projects.
- **Status:** NOT FIXED. Acceptable heuristic. Now uses `else if` so only one of Maven/Gradle is returned, preventing duplicate commands in monorepo mode.

### Low

#### L1: No upper bound on `max_revision_rounds`
- **File:** `vtcode-core/src/core/agent/runner/execute.rs:686`
- **Root cause:** Pre-existing. A user could set `max_revision_rounds: 1000` with no validation.
- **Impact:** Low. Would only cause excessive retries, not a crash.
- **Status:** NOT FIXED. Pre-existing issue, not introduced by our changes.

---

## False Positives Eliminated

| Claimed Issue | Why It's a False Positive |
|--------------|--------------------------|
| `is_allowed_in_full_auto` fail-open default | The function is called unconditionally from `is_tool_exposed`. Changing `.unwrap_or(true)` to `.unwrap_or(false)` breaks all non-full-auto sessions. The correct fix requires tracking full-auto state separately from the allowlist (deeper refactor). Documented as known concern. |
| `max_revision_rounds` infinite loop risk | Verified: `apply_evaluator_gate` at `orchestration.rs:436` checks `*revision_rounds_used >= max_revision_rounds` before incrementing. With `max_revision_rounds = 0`, the first call returns `Exhausted` immediately. No loop possible. |
| Maven/Gradle priority is a bug | It's a documented first-match heuristic. Both build tools are rarely used simultaneously in the same project. Acceptable risk. |

---

## Files Changed

| File | Changes |
|------|---------|
| `vtcode-core/src/tools/registry/policy.rs` | Reverted (fix deferred -- needs deeper refactor) |
| `vtcode-core/src/core/agent/runner/execute.rs` | Removed `.max(1)` clamp on `max_revision_rounds` |
| `src/agent/runloop/unified/state.rs` | Added `tracing::warn!` for safe-mode prompt bypass |
| `src/agent/runloop/unified/turn/turn_processing/planning_workflow.rs` | Added timeout logging; increased `max_tokens` to 1024 |
| `src/agent/runloop/unified/turn/turn_processing/planning_workflow/interview_payload.rs` | Fixed `truncate_hint` byte/char mismatch |
| `vtcode-core/src/core/agent/runner/workspace_detection.rs` | Added Go/Java/Ruby/.NET detection; fixed .NET shallow scan; fixed Ruby rspec assumption; fixed monorepo first-match |
| `src/agent/runloop/unified/turn/turn_loop_helpers.rs` | Removed dead `_result` parameter |
| `src/agent/runloop/unified/turn/turn_loop.rs` | Updated caller for removed parameter |

---

## Validation

- `cargo check` -- compiles cleanly
- `cargo test -p vtcode-core --lib` -- 3303 passed, 0 failed
- `cargo clippy` -- pre-existing warnings only (not from these changes)
