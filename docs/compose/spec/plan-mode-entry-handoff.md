---
feature: plan-mode-entry-handoff
status: in-progress
updated: 2026-09-27
branch: fix/plan-mode-entry-handoff
commits: # leave empty while in progress; fill at delivery
---

# Plan Mode Entry Handoff (Seamless Build → Plan Switch)

## Report

## [S1] Problem

When an execution agent (Build/Auto) calls `start_planning` and the user confirms
**Enter Planning workflow**, the session shows the Plan header and the
"Drafting plan — researching codebase" indicator, then **stops**. No research
tools run, no `<proposed_plan>` is drafted, and the session waits at
`Continue…`. Plan mode is stuck.

Session evidence (`session-vtcode-20260927T030348Z_353055-84925`, 13 events):

1. `start_planning` tool succeeds with `status: success` and planning instructions.
2. `turn.completed` fires immediately after the tool result.
3. No step-2 research calls. The earlier working session
   (`session-vtcode-20260927T023955Z_014451-52652`) continued `exec_command` /
   `code_search` in the **same** turn after `start_planning`.

Commit `a34d5bfca` (Reapply "Merge branch 'fix/plan-mode-header-handoff'")
introduced the regression:

- `handle_start_planning` sets `ToolPipelineOutcome.pending_primary_agent = "plan"`.
- `execution_result.rs` converts any `pending_primary_agent` into
  `TurnHandlerOutcome::SwitchPrimaryAgent`, which **breaks the turn**.
- Research never starts, despite docs stating "read-only research begins" and
  "the plan primary agent is selected **after the turn**".

The approval return path (plan → Build/Auto) is a separate handoff and is not
this bug; it must keep working after the fix.

## [S2] Design

### A. Plan-entry contract (mid-turn `start_planning`)

Entering planning from Build/Auto must:

1. Enable the planning workflow and block mutating tools (existing
   `transition_to_planning_workflow`).
2. Refresh the session header badge to Plan **immediately**
   (`apply_plan_agent_header`).
3. **Continue the current turn** so the model can research and draft
   `<proposed_plan>`. Do **not** emit `SwitchPrimaryAgent` / end the turn.
4. Queue a **deferred** full switch to the `plan` primary agent for
   **after the turn naturally completes** (prompt/tools match Plan for
   subsequent planning turns).
5. On plan approval, hand off to a write-capable Build/Auto agent through the
   existing typed `PlanExecutionTarget` path and return to implementation mode.

### B. Deferred switch mechanics

- `PlanningWorkflowSessionState` gains `plan_entry_agent_switch_pending: bool`
  (cleared on `enter`/`exit`/take).
- `enter_planning_workflow_after_start` sets the flag and updates the header;
  it must **not** set `ToolPipelineOutcome.pending_primary_agent`.
- After final-response validation in `turn_loop` (so a normal completed planning
  turn still requires a published final), take the flag and set
  `TurnLoopOutcome.pending_primary_agent = Some("plan")` only if no stronger
  handoff is already present (`SwitchPrimaryAgentWithPolicy` / approved-plan).
- Orchestration's existing `is_plan_entry_handoff` branch selects the plan agent
  (not the write-capable resolver) and must not set
  `plan_approved_execution_pending`.

### C. Mode-switch error handling and logging

Every plan-entry / plan-exit / approval handoff path must:

1. Catch selection/sync failures without panicking or silently dropping the
   session into a half-switched state.
2. Log with `tracing` at `warn!`/`error!` including: path
   (`plan_entry` / `plan_approval` / `plan_exit` / `startup_plan_entry`),
   requested agent, resolved agent (when known), and the error.
3. On plan-entry switch failure: keep Planning workflow + Plan header, continue
   the turn (research still allowed under the read-only gate), and surface a
   single warning line. Do not abort the turn.
4. On plan-approval switch failure: keep the approved plan, surface an error
   line, and leave the session able to retry approval / `/mode build`.
5. Emit `tracing::info!(target: "vtcode.planning_workflow", ...)` on successful
   switches (already present for approval; add matching success logs for entry
   deferred-switch and `/plan off` restore).

### D. Out of scope

- Plan validation, tracker distill, or approval policy changes.
- Build vs Auto authority rules.
- Header layout / badge styling.
- `/plan` slash-command entry (already performs a full select; leave unless a
  shared helper is extracted without behavior change).

## [S3] Out of Scope

See [S2] D. No new top-level harness subsystem. No `ThreadEvent` contract
changes beyond existing plan-approval / planning lifecycle events.

## Tasks

- [ ] T1: Stop plan-entry from breaking the turn — remove
  `pending_primary_agent` from `start_planning` outcomes and add deferred
  `plan_entry_agent_switch_pending` — acceptance: after confirmed
  `start_planning`, the turn continues and can emit research tool calls
  (covers: S2 A,B)
- [ ] T2: Apply deferred plan-agent switch after natural turn completion,
  without suppressing final-response validation — acceptance: a completed
  planning turn with a published final still validates the final, then
  orchestration selects `plan` (covers: S2 A,B)
- [ ] T3: Harden mode-switch error handling and logging on
  entry/approval/exit/startup paths — acceptance: failed selection logs
  path+error and leaves a recoverable session; success paths log
  `vtcode.planning_workflow` (covers: S2 C)
- [ ] T4: Regression tests — plan-entry does not set `SwitchPrimaryAgent`
  mid-turn; deferred switch applies at turn end; plan-approval still hands off
  to Build/Auto; entry failure keeps planning active — acceptance:
  `cargo nextest run -p vtcode -E '...planning/mode...'` includes these cases
  and passes (covers: S2 A–C)
- [ ] T5: Update planning-workflow docs for deferred-switch + error contract —
  acceptance: docs state research continues in the entry turn and the plan
  agent is selected after the turn (covers: S2 A–C)
