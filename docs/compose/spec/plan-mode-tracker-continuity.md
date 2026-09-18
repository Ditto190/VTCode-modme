---
feature: plan-mode-tracker-continuity
status: in-progress
updated: 2026-09-18
branch: feat/plan-mode-tracker-continuity
commits: # filled at delivery
---

# Plan Mode + Tracker Continuity (Dedupe, Scope/Task, Continuation, Progress)

## Report

## [S1] Problem

Two related product gaps remain after `tracker-user-facing-ui`:

1. **Duplicated TODO/task transcript blocks.** Evidence: `Screenshot 2026-09-17 at 13.45.47.png` shows two identical full tracker trees stacked in the transcript (`• Tasks 0/5 — next: …` + the same checklist rows twice) before “Now applying the edit”. Root cause class: tracker transcript writes are not single-writer. Approval handoff uses `append_pasted_message` and does **not** update `HarnessTurnState.replaceable_task_tracker_block`; the pipeline path uses `apply_task_tracker_block`, which appends when `replace_count` is `None` and `transcript::tail_matches` is false (counts/next-step text changed). Slightly different progress text therefore stacks a second block instead of replacing the first. The old full-tree format made the bug obvious; progress-only lines can still stack.

2. **Plan-mode scope/task/continuation/progress.** The `<proposed_plan>` template has Summary / Implementation Steps / Test Cases / Assumptions but no explicit **Scope** contract, so “what is in/out” is easy to lose when distilling to `task_tracker`. Plan mode remains terminal for tracker auto-continue (`should_queue_tracker_auto_continue` returns false when `planning_active`), so budget/recovery ends still nudge the user even when planning work is incomplete and no decision is required. User-facing plan progress is not aligned with the title+progress tracker contract (open decisions, step readiness, approval state).

## [S2] Design

Decision (user-confirmed): one follow-up package covering all three plan-mode outcomes **plus** transcript dedupe.

### A. Single-writer tracker transcript (dedupe)

**Contract**

- Exactly **one** user-facing tracker block lives in the transcript at a time.
- All tracker transcript writes go through one helper; no raw `append_pasted_message` for tracker progress/tree.

```rust
// binary: tool_output_handler / shared helper
fn write_tracker_transcript_block(handle: &InlineHandle, lines: &[String])
```

Behavior:

1. If transcript tail already equals `lines` → remember and return (no stack).
2. Else if a remembered replaceable tracker block exists (session-scoped, not only `HarnessTurnState`) → `replace_last(remembered_len)` + write `lines`.
3. Else if transcript tail looks like a previous tracker progress/header block → replace that tail.
4. Else append `lines`.
5. Always remember `lines` as the current replaceable block.

**Shared memory**

- Move replaceable-block tracking out of turn-local `HarnessTurnState` into the session transcript utility (`vtcode_core::utils::transcript`) so approval handoff and pipeline share one source of truth.
- `HarnessTurnState` may keep a thin alias for tests, but production writes must hit the shared store.

**Call sites**

- `tool_output_handler` task_tracker path
- `planning_workflow/task_tracker.rs` `render_created_task_tracker`
- Any other tracker transcript write

**Acceptance**

- Approval create + pipeline update + follow-up update → **one** progress line in transcript.
- Two successive different progress values → replace, not append.
- Identical repeats → no extra command.

### B. Plan template: Scope / Task map

**Template (`docs/guides/planning-workflow.md` + compiled planning prompt if present)**

Add required section:

```markdown
## Scope

- In: [concrete surfaces/behaviours this plan changes]
- Out: [explicit non-goals; must match Assumptions when material]
```

Keep existing required sections. Validator: `Scope` becomes required; existing plans without it still validate until a deprecation note (or accept optional with warning — **choose required for new drafts**; runtime validation update in `validate_plan_content`).

**Distill → tracker**

- `task_items_from_plan` continues to take **Implementation Steps** / **Steps` only for checklist items (unchanged).
- `Scope.Out` is **not** turned into tracker steps; it is preserved on the plan artifact and may appear as a user-facing “out of scope” note in plan progress (not as TODO rows).
- Step descriptions still strip `-> files:` / `-> verify:` into structured metadata.

**Acceptance**

- New plan template docs + prompt guidance include Scope In/Out.
- Distill test: Scope lines never become tracker items; Implementation Steps do.

### C. Plan-mode continuation

**New pure gate** (sibling of tracker gates):

```rust
pub(crate) fn should_queue_plan_mode_auto_continue(
    auto_continue_enabled: bool,
    planning_active: bool,
    plan_ready_for_approval: bool,
    awaiting_user_decision: bool, // interview / approval / permission
    turn_completed: bool,
    blocked_reason: Option<&str>,
    is_verification_block: bool,
    cross_turn_turns: u8,
) -> bool
```

Rules:

- `false` when planning inactive, kill-switch off, `cross_turn_turns == 0`, user decision/approval wait, or verification block.
- `false` when `plan_ready_for_approval` (approval is a user gate — do not auto-approve).
- `true` when planning active, plan **not** ready, no user decision needed, and turn completed **or** recoverable blocked reason (reuse production blocked-reason constants).
- Budget: reuse `[agent.harness.continuation].cross_turn_turns` or add `plan_cross_turn_turns` — **recommend reuse** with a dedicated reset on approval/exit planning.

**Outer loop**

- After planning turn finalization, if gate passes → queue one planning continuation prompt: continue research/synthesis toward a validated `<proposed_plan>`; do not ask the user to resume; do not implement.
- Mirror tracker auto-queue: queue-first, Info line, continue session loop; queue-full falls through to blocked handoff.

**In-turn**

- Planning status recaps that are not user questions may force-continue when a plan is not yet ready (optional parity with tracker override) — only if cheap and gated; outer-loop auto-queue is the primary fix.

**Acceptance**

- Unit tests: no queue when awaiting approval/interview; queue when planning incomplete + recoverable budget end; no implementation handoff.

### D. Plan progress presentation

While `planning_active` and no approved tracker yet, user-facing status/transcript line:

```text
• Plan <humanized title> — research/synthesis
• Plan <humanized title> — open decisions: N
• Plan <humanized title> — ready for approval (M steps)
```

- No full research dump; no internal tool args.
- After approval/handoff, existing tracker progress contract applies (`• <tracker title> N/M`).
- Docked panel may show plan steps when a planning tracker exists; visibility rules unchanged.

**Acceptance**

- Tests for progress line shapes for research / open-decision / ready-for-approval.
- Docs updated in planning-workflow + interactive-mode.

### Config

```toml
[agent.harness.continuation]
auto_continue_tracker = true      # existing
cross_turn_turns = 8              # existing; also bounds plan-mode auto-continue
# optional alias if split budget desired later — not required for v1
```

### Out of Scope

- Auto-approving plans or auto-implementing from plan mode.
- Changing tool permission policy or planning read-only enforcement.
- Redesigning TODO panel chrome beyond progress/title contracts already delivered.
- Infinite planning auto-run — bounded by `cross_turn_turns`, hard caps, Esc/interrupt.

## Tasks

- [ ] T1: Session-scoped replaceable tracker transcript block + `write_tracker_transcript_block` — acceptance: unit tests cover identical skip, replace on change, append when none (covers: S2A)
- [ ] T2: Route all tracker transcript writes through the helper (pipeline + approval) — acceptance: integration-style tests show one block after approval+pipeline+update (covers: S2A; depends: T1)
- [ ] T3: Plan template Scope section + validator/prompt guidance — acceptance: docs/prompt include Scope In/Out; distill ignores Scope for tracker items (covers: S2B)
- [ ] T4: `should_queue_plan_mode_auto_continue` + outer-loop wiring — acceptance: pure tests for approval-wait / recoverable / kill-switch; no auto-approve (covers: S2C)
- [ ] T5: Plan progress presentation lines + docs — acceptance: shape tests + planning-workflow/interactive-mode updates (covers: S2D; depends: T3)
- [ ] T6: Feature-doc finalize + AGENTS/docs map if presentation contract changes — acceptance: shipped surfaces documented (covers: S2; depends: T2,T4,T5)
