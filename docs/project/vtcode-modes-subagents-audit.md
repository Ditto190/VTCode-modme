# VTCode Modes and Subagents: Comprehensive Audit Report

**Date:** 2026-06-20
**Scope:** `plan`, `build`, and `auto` primary modes; subagent system architecture
**Status:** Final

---

## Executive Summary

VTCode implements a mode system based on named primary agents (`SubagentSpec` instances) with distinct permission profiles. The architecture is sound in principle but has several concrete issues: an orchestration layer that only works in headless mode, a fail-open security default in the auto mode tool policy, fragmented planning workflow state, and silent config clamping that overrides user intent. This audit identifies 14 blockers and issues across the three modes, proposes an optimized architecture, and defines a validation process.

---

## 1. Architecture Overview

### 1.1 Mode Definitions

Modes are defined as builtin `SubagentSpec` instances in `vtcode-config/src/subagents.rs`:

| Mode | Function | Line | PermissionDefault | AgentMode | Tools |
|------|----------|------|-------------------|-----------|-------|
| `build` | `builtin_primary_build_agent()` | 554 | `Ask` | `Primary` | All (full tool set) |
| `auto` | `builtin_primary_auto_agent()` | 587 | `Auto` | `Primary` | All (full tool set) |
| `plan` | `builtin_plan_agent()` | 620 | `Deny` | `All` | Read-only subset |
| `duck` | `builtin_primary_duck_agent()` | 649 | `Deny` | `Primary` | Read-only subset |

### 1.2 Mode Selection Flow

```
CLI args
  -> action_resolution.rs: resolve_action()
    -> --print/-p       => ResolvedCliAction::Ask
    -> --full-auto/-auto => ResolvedCliAction::FullAuto
    -> --resume         => ResolvedCliAction::Resume
    -> subcommand       => ResolvedCliAction::Command
    -> (default)        => ResolvedCliAction::Chat
  -> dispatch() routes to handler
    -> FullAuto => auto::handle_auto_task_command() => AgentRunner with auto primary agent
    -> Chat     => handle_chat_command() => UnifiedSessionRuntime with default primary agent
    -> Ask      => handle_ask_single_command() => one-shot LLM call
```

Within interactive sessions, users cycle primary agents via `Tab`/`Shift+Tab`.

### 1.3 Subagent Discovery Pipeline

Defined in `discover_subagents()` at `vtcode-config/src/subagents.rs:395`:

```
Priority 0 (highest): CLI agents
Priority 1:           .vtcode/agents/ (project)
Priority 2:           .claude/agents/ (project)
Priority 3:           .codex/agents/ (project)
Priority 4:           ~/.vtcode/agents/ (user)
Priority 5:           ~/.claude/agents/ (user)
Priority 6:           ~/.codex/agents/ (user)
Priority 7:           Plugin agents
Priority 8 (lowest):  Builtin agents (always shadowed)
```

### 1.4 Subagent Runtime

Subagents are spawned via `SubagentController::spawn()` at `vtcode-core/src/subagents/mod.rs:665`, which:
1. Resolves the spec from discovered agents
2. Prepares delegation prompt
3. Checks concurrency limits (default: 3 concurrent, 1 depth)
4. Creates a `ChildRecord` and launches a tokio task running `child_loop()`

---

## 2. Identified Issues

### 2.1 CRITICAL: Orchestration Mode Silently Ignored in Interactive Path

**File:** `src/agent/runloop/unified/turn/turn_loop.rs`
**Impact:** Users who configure `orchestration_mode = "plan_build_evaluate"` get no effect in interactive sessions.

The `PlanBuildEvaluate` orchestration is implemented exclusively in the exec runner (`vtcode-core/src/core/agent/runner/orchestration.rs`). The interactive turn loop in `src/agent/runloop/unified/turn/` has zero references to `orchestration_mode`, `HarnessOrchestrationMode`, or `harness_plan_build_evaluate`. The guard function at `orchestration.rs:263-275` requires `full_auto_active == true`, meaning orchestration only fires in headless/exec mode.

The config field at `vtcode-config/src/core/agent.rs:390` has no documentation stating this scope limitation.

**Recommendation:** Either wire orchestration into the interactive path, or document the limitation explicitly and warn at config parse time when `orchestration_mode != Single` in interactive contexts.

---

### 2.2 CRITICAL: Fail-Open Default in Auto Mode Tool Policy

**File:** `vtcode-core/src/tools/registry/policy.rs:296-302`

```rust
pub fn is_allowed_in_full_auto(&self, name: &str) -> bool {
    self.full_auto_allowlist
        .as_ref()
        .map(|allowlist| allowlist.contains(canonical))
        .unwrap_or(true)  // <-- fail-open when allowlist is None
}
```

When `full_auto_allowlist` is `None` (i.e., `enable_full_auto` was never called), all tools are permitted. While all current call sites (`auto.rs:111`, `exec/run.rs:168`) do call `enable_full_auto`, any future code path that creates a runner without it would have unrestricted tool access.

**Recommendation:** Change `.unwrap_or(true)` to `.unwrap_or(false)`. The fail-closed default is safer; code paths that need full access must explicitly opt in.

---

### 2.3 HIGH: `max_revision_rounds` Silently Clamped to Minimum 1

**File:** `vtcode-core/src/core/agent/runner/execute.rs:686`

```rust
self.config().agent.harness.max_revision_rounds.max(1)
```

A user who sets `max_revision_rounds = 0` (expecting "no revision, fail immediately on evaluator rejection") silently gets 1 revision round. There is no validation warning in `AgentConfig::validate_llm_params()` (`agent.rs:675-699`).

**Recommendation:** Either respect the user's value of 0 (meaning zero revision rounds), or add a config validation warning that clamps it to 1 and logs the override.

---

### 2.4 HIGH: Planning Workflow State Fragmentation

**Files:**
- `src/agent/runloop/unified/planning_workflow_state.rs` (session-level state)
- `vtcode-core/src/tools/handlers/planning_workflow.rs` (tool-level state)
- `vtcode-core/src/tools/registry/safety_gateway.rs` (permission gating)
- `src/codex_app_server/runtime.rs` (duplicate normalization)

The planning concept is spread across 4+ files with no single source of truth. `PlanningWorkflowSessionState` tracks interview cycles at the session level, while `PlanningWorkflowState` in the tool registry tracks phase and plan file at the tool level. The `normalize_planning_input` function in `runtime.rs` duplicates logic that exists elsewhere.

**Recommendation:** Consolidate planning state into a single module. The session-level `PlanningWorkflowSessionState` should own the lifecycle; the tool-level state should be a derived view.

---

### 2.5 HIGH: `is_allowed_in_full_auto` Fail-Open Combined with Persistent Trust

**Files:**
- `vtcode-core/src/tools/registry/policy.rs:301` (fail-open)
- `src/startup/workspace_trust.rs:122-124` (persistent trust, no expiry)

Once workspace trust is granted (via `VTCODE_TRUST_WORKSPACE=1` or interactive prompt), it is persisted to disk and never expires. Combined with the fail-open tool policy, a stale trust entry could allow unrestricted auto mode execution in a workspace that is no longer trusted.

**Recommendation:** Add trust expiry (e.g., 30 days) and a `vtcode trust revoke` command. Change the tool policy default to fail-closed.

---

### 2.6 MEDIUM: Silent Safe-Mode Prompt Bypass in Auto Mode

**File:** `src/agent/runloop/unified/state.rs:444-454`

When `full_auto` is true, `should_enforce_safe_mode_prompts` returns `false`, disabling forced prompts for high-risk tools. This is intentional but undocumented -- users may not realize that auto mode disables safety prompts entirely.

**Recommendation:** Log a one-time warning when safe-mode prompts are bypassed. Document this behavior in the `--auto`/`--full-auto` CLI help text.

---

### 2.7 MEDIUM: Interview Synthesis Timeout Silently Swallowed

**File:** `src/agent/runloop/unified/turn/turn_processing/planning_workflow.rs:190-194`

The LLM call for interview synthesis is wrapped in `tokio::time::timeout(Duration::from_secs(20))`. On timeout, the error is silently swallowed and falls back to `build_adaptive_fallback_interview_args`. No logging occurs.

**Recommendation:** Log a `warn!` when the interview synthesis times out so operators can diagnose degraded interview quality.

---

### 2.8 MEDIUM: Interview Synthesis Token Budget May Be Too Low

**File:** `src/agent/runloop/unified/turn/turn_processing/planning_workflow.rs:187`

`max_tokens: Some(700)` for interview synthesis may truncate complex payloads (3 questions x 3 options + analysis_hints + focus_area + question text). Truncation causes JSON parse failure, which falls back to the adaptive fallback interview.

**Recommendation:** Increase to 1024 or make it configurable. Add a metric/log when fallback is used due to parse failure.

---

### 2.9 MEDIUM: `truncate_hint` Byte/Char Length Mismatch

**File:** `src/agent/runloop/unified/turn/turn_processing/planning_workflow/interview_payload.rs:462-473`

`truncate_hint` uses `trimmed.len()` (byte length) for the initial check but `chars().take()` (char count) for truncation. Multi-byte UTF-8 hints could pass the byte-length check but produce different truncation behavior.

**Recommendation:** Use consistent measurement -- either both byte-length or both char-count.

---

### 2.10 MEDIUM: Duplicate Tracker Block Handling

**File:** `src/agent/runloop/unified/turn/turn_processing/planning_workflow/interview_context.rs:329-341`

`extract_embedded_tracker` uses `find(PLAN_TRACKER_START)` which finds only the first occurrence. If plan content contains duplicate tracker blocks (from a bad append), only the first is processed; the rest remain embedded.

**Recommendation:** Use `rfind` for the last occurrence, or validate that exactly one tracker block exists and warn on duplicates.

---

### 2.11 LOW: Dead `_result` Parameter in Planning Exit Trigger

**File:** `src/agent/runloop/unified/turn/turn_loop_helpers.rs:342`

The `_result: &mut TurnLoopResult` parameter is prefixed with underscore and never used. The caller at `turn_loop.rs:408-411` passes it but never reads it after the call.

**Recommendation:** Remove the parameter or wire it into the function logic.

---

### 2.12 LOW: `TurnLoopContext` Struct Bloat (32 Fields)

**File:** `src/agent/runloop/unified/turn/turn_loop.rs:98-140`

The struct has 32 fields with `#[expect(clippy::too_many_arguments)]` on `new()`. This makes it difficult to add orchestration awareness without further inflation.

**Recommendation:** Group related fields into sub-structs (e.g., `PlanningContext`, `SafetyContext`, `RecoveryContext`).

---

### 2.13 LOW: Legacy Alias Strings in `HarnessOrchestrationMode::parse()`

**File:** `vtcode-config/src/core/agent.rs:316-317`

Two undocumented legacy aliases (`"planner_generator_evaluator"`, `"planner-generator-evaluator"`) are accepted. They are tested once and never referenced in docs or config files.

**Recommendation:** Remove the legacy aliases or document them as deprecated.

---

### 2.14 LOW: Limited Workspace Detection for Verify Commands

**File:** `vtcode-core/src/core/agent/runner/workspace_detection.rs`

Only detects Rust, Python, and Node.js. Missing Go, Java, Ruby, .NET. For unsupported ecosystems, the planner phase produces tracker items with no verification commands unless the LLM explicitly provides them.

**Recommendation:** Add detection for Go (`go.mod`), Java (`pom.xml`/`build.gradle`), Ruby (`Gemfile`), .NET (`*.csproj`).

---

## 3. Per-Mode Analysis

### 3.1 Plan Mode

**Entry:** Tab-cycling to `plan` primary agent, or `/plan` slash command, or phrases like "make a plan"
**Exit:** User says "implement", "yes", "go", "start", or similar
**State machine:** `PlanningWorkflowSessionState` (session) + `PlanningWorkflowState` (tool registry)

**Strengths:**
- Read-only enforcement via `PermissionDefault::Deny` with explicit allow list
- Interview workflow forces structured requirements gathering before implementation
- Stay-phrase exclusions prevent accidental exits ("do not implement")
- `AgentMode::All` allows plan to be used as both primary and subagent

**Weaknesses:**
- Exit detection uses substring matching which can produce false positives in edge cases
- "continue" requires the assistant to have recently prompted for implementation; standalone "continue" is a no-op
- Interview synthesis has a silent 20s timeout with no logging
- `strip_assistant_text` converts `TextResponse` to `Empty`, restructuring the message type

### 3.2 Build Mode

**Entry:** Default mode for interactive sessions (`vtcode` with no subcommand)
**Permission model:** `PermissionDefault::Ask` -- user confirms mutations

**Strengths:**
- Interactive confirmation for sensitive operations
- Safe-mode prompts enforced for high-risk tools
- Full tool access with human-in-the-loop gating
- `unified_exec` explicitly allowed via tool policy override

**Weaknesses:**
- `full_auto_allowlist` is `None` in build mode, so `is_allowed_in_full_auto` returns `true` for all tools (fail-open). This is mitigated by safe-mode prompts but is architecturally fragile.
- No orchestration support -- the `PlanBuildEvaluate` cycle is unavailable in interactive build mode
- `TurnLoopContext` bloat makes it hard to add new capabilities

### 3.3 Auto Mode

**Entry:** `vtcode --full-auto "prompt"` or `vtcode --auto "prompt"`
**Permission model:** `PermissionDefault::Auto` -- no user confirmation

**Strengths:**
- Workspace trust gate prevents accidental auto mode
- Full-auto allowlist restricts available tools
- `Prompt` policy tools still require human-in-the-loop even in auto mode
- PlanBuildEvaluate orchestration provides structured execution with evaluator gating
- Retry with exponential backoff for transient failures

**Weaknesses:**
- Fail-open tool policy default (Issue 2.2)
- Persistent trust without expiry (Issue 2.5)
- Safe-mode prompts silently bypassed (Issue 2.6)
- Budget enforcement disabled for unknown models (execute.rs:930-945)
- `max_revision_rounds` silently clamped to 1 (Issue 2.3)
- Wildcard tool allowance (`"*"`) grants all tools (Issue 2.5)

---

## 4. Proposed Optimized Architecture

### 4.1 Centralize Planning State

```
Current:                          Proposed:
planning_workflow_state.rs        planning/
  PlanningWorkflowSessionState      mod.rs
planning_workflow.rs                state.rs        -- single PlanningState struct
interview_context.rs                interview.rs    -- interview synthesis + forcing
interview_forcing.rs                context.rs      -- research context + plan draft
interview_payload.rs                payload.rs      -- parsing + sanitization
safety_gateway.rs (planning check)  transitions.rs  -- enter/exit triggers
runtime.rs (normalize_planning_input)
```

The `PlanningState` struct would own:
- Interview cycle tracking (currently in `PlanningWorkflowSessionState`)
- Phase tracking (currently in `PlanningWorkflowState`)
- Plan file path and baseline
- Entry source

### 4.2 Wire Orchestration Into Interactive Path (Optional)

Add a config flag `orchestration_mode.interactive = true` that enables PlanBuildEvaluate in the interactive turn loop. This would:
1. After plan mode exit, automatically run the planner phase
2. During build, track against the spec/contract
3. After build, run the evaluator phase

This is a significant change and should be gated behind a feature flag.

### 4.3 Fix Security Defaults

```
is_allowed_in_full_auto:  .unwrap_or(true)  ->  .unwrap_or(false)
trust expiry:             never              ->  30 days, revocable
safe-mode bypass:         silent             ->  logged warning
max_revision_rounds:      clamped to 1       ->  validated with warning
```

### 4.4 Consolidate Tool Catalog Cache Invalidation

Currently, `note_explicit_refresh` is called in multiple places. Centralize it into a single `invalidate_tool_catalog()` method on `ToolRegistry` that is called on every mode transition.

### 4.5 Expand Workspace Detection

Add Go, Java, Ruby, and .NET detection to `workspace_detection.rs`. Use a simple file-presence check:

```rust
fn detect_go_verify_commands(workspace: &Path) -> Option<Vec<String>> {
    if workspace.join("go.mod").exists() {
        Some(vec!["go test ./...".to_string()])
    } else {
        None
    }
}
```

---

## 5. Validation Process

### 5.1 Plan Mode Validation

| Test | Expected | Verification |
|------|----------|--------------|
| Enter via "make a plan" | Planning workflow activates | `is_planning_active() == true` |
| Enter via `/plan` | Planning workflow activates | `is_planning_active() == true` |
| Mutating tools blocked | `write_file`, `exec` denied | Tool call returns `PermissionDenied` |
| Read-only tools work | `read_file`, `search` succeed | Tool call returns `Success` |
| Interview shown after plan draft | Interview tool call injected | Response contains interview tool call |
| "implement" exits planning | Planning workflow deactivated | `is_planning_active() == false` |
| "do not implement" stays | Planning workflow remains active | `is_planning_active() == true` |
| "continue" with no prompt | No-op, hint displayed | `is_planning_active() == true` |
| "continue" after prompt | Exits planning | `is_planning_active() == false` |
| Interview synthesis timeout | Fallback interview used | Log warning emitted |

### 5.2 Build Mode Validation

| Test | Expected | Verification |
|------|----------|--------------|
| Default primary agent is `build` | `active_primary_agent.name == "build"` | Session init log |
| Mutating tools require confirmation | User prompted for `write_file` | TUI shows confirmation dialog |
| Read-only tools auto-approve | `read_file` executes without prompt | No confirmation dialog |
| Tab cycling works | Cycles through primary agents | TUI shows agent switch |
| Subagent spawning respects permissions | Child inherits parent's tool set | Child tool call behavior |
| Safe-mode prompts enforced | High-risk tools require confirmation | TUI shows confirmation |

### 5.3 Auto Mode Validation

| Test | Expected | Verification |
|------|----------|--------------|
| Empty prompt rejected | Error message | Exit code 1 |
| Untrusted workspace rejected | Trust prompt shown | Interactive prompt |
| `automation.full_auto.enabled = false` | Error message | Exit code 1 |
| Auto agent selected | `active_primary_agent.name == "auto"` | Runner config log |
| Full-auto allowlist active | Only allowed tools execute | Denied tools return error |
| Prompt policy tools still require approval | `Prompt` tools gated | Tool call returns `Prompt` |
| PlanBuildEvaluate runs | Planner -> Generator -> Evaluator | Harness events emitted |
| Evaluator rejection triggers revision | Revision round incremented | `RevisionStarted` event |
| Max revision rounds exhausted | Task fails with reason | `Exhausted` outcome |
| Budget enforcement active | Stops at budget limit | Budget exceeded error |
| Retry on transient failure | Exponential backoff | Retry log messages |

### 5.4 Cross-Mode Validation

| Test | Expected | Verification |
|------|----------|--------------|
| Mode switch invalidates tool catalog | Fresh tool set after switch | Tool list changes |
| Subagent permissions inherit from parent | Child respects parent's mode | Child tool behavior |
| Planning state resets on mode change | No stale planning state | `is_planning_active() == false` |
| Trust level persists across sessions | Trusted workspace stays trusted | Trust file on disk |
| Config changes require restart | Mode config changes are not hot-reloaded | Error on hot-reload attempt |

---

## 6. Priority Matrix

| Priority | Issue | Effort | Risk |
|----------|-------|--------|------|
| P0 | Fix fail-open tool policy default (2.2) | Small | Security |
| P0 | Fix silent config clamping (2.3) | Small | Correctness |
| P1 | Centralize planning state (2.4) | Medium | Maintainability |
| P1 | Add trust expiry (2.5) | Medium | Security |
| P1 | Log safe-mode bypass (2.6) | Small | Observability |
| P2 | Log interview synthesis timeout (2.7) | Small | Observability |
| P2 | Increase interview token budget (2.8) | Small | Quality |
| P2 | Fix byte/char mismatch (2.9) | Small | Correctness |
| P2 | Handle duplicate tracker blocks (2.10) | Small | Correctness |
| P2 | Document orchestration scope (2.1) | Small | Documentation |
| P3 | Remove dead parameter (2.11) | Small | Code quality |
| P3 | Reduce TurnLoopContext bloat (2.12) | Large | Maintainability |
| P3 | Remove legacy aliases (2.13) | Small | Code quality |
| P3 | Expand workspace detection (2.14) | Small | Feature coverage |

---

## 7. File Reference Index

| Component | File | Key Lines |
|-----------|------|-----------|
| Builtin mode definitions | `vtcode-config/src/subagents.rs` | 554-676 |
| Permission helpers | `vtcode-config/src/subagents.rs` | 698-710 |
| Agent mode enum | `vtcode-config/src/subagents.rs` | 136-143 |
| Discovery pipeline | `vtcode-config/src/subagents.rs` | 395-465 |
| CLI action resolution | `src/cli/action_resolution.rs` | 1-47 |
| CLI dispatch | `src/cli/mod.rs` | 54-117 |
| Auto mode handler | `src/cli/auto.rs` | 20-157 |
| Auto agent resolution | `src/cli/full_auto_primary_agent.rs` | 15-54 |
| Planning state | `src/agent/runloop/unified/planning_workflow_state.rs` | 9-83 |
| Planning transitions | `src/agent/runloop/unified/planning_workflow_state.rs` | 112-152 |
| Turn loop helpers (enter/exit) | `src/agent/runloop/unified/turn/turn_loop_helpers.rs` | 338-675 |
| Interview forcing | `src/agent/runloop/unified/turn/turn_processing/planning_workflow/interview_forcing.rs` | 1-254 |
| Interview context | `src/agent/runloop/unified/turn/turn_processing/planning_workflow/interview_context.rs` | 74-341 |
| Interview payload | `src/agent/runloop/unified/turn/turn_processing/planning_workflow/interview_payload.rs` | 14-473 |
| Orchestration mode enum | `vtcode-config/src/core/agent.rs` | 296-300 |
| Orchestration implementation | `vtcode-core/src/core/agent/runner/orchestration.rs` | 262-460 |
| Task execution | `vtcode-core/src/core/agent/runner/execute.rs` | 632-667 |
| Tool policy gateway | `vtcode-core/src/tools/registry/policy.rs` | 257-348 |
| Tool access checks | `vtcode-core/src/core/agent/runner/tool_access.rs` | 139-191 |
| Workspace trust | `src/startup/workspace_trust.rs` | 84-198 |
| Safe-mode prompts | `src/agent/runloop/unified/state.rs` | 444-454 |
| Workspace detection | `vtcode-core/src/core/agent/runner/workspace_detection.rs` | - |
