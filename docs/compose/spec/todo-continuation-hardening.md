---
feature: todo-continuation-hardening
status: delivered
updated: 2026-09-19
branch: fix/todo-continuation-gaps
commits: 326810f14..fc5e3851d
---

# TODO Continuation Hardening (Residual)

Residual work after delivered `tracker-continuation`. Evidence session `session-vtcode-20260918T030054Z_141498-26410` auto-continued 20 turns with zero user `continue` prompts, but the harness and model still present “stopped / take action” to the user, and several production blocked-reasons still fall through to `Type continue`.

## Report

**What was built** — VT Code’s TODO/tracker continuation is hardened so incomplete `task_tracker` work continues across turns without user nudges, while true handoffs still stop for the user.

1. **Classifiers.** In-turn recoverable phrasing, outer `tracker_auto_continue_is_recoverable_block`, and `tracker_final_text_is_safety_handoff` now treat tool-call/tool-loop/read-cap/budget-exhausted recaps as recoverable, not policy denials. `plan_mode_recoverable_block` is allow-list first so production `PLANNING_COMPLETED_TURN_FALLBACK_REASON` auto-queues instead of emitting “Type continue”. Bare `"tool policy"` over-match is narrowed to explicit denials.

2. **In-turn override.** Applies on tool-free recovery status text; gated only on `auto_continue_tracker` (not `cross_turn_turns > 0`). When tracker work remains, only trailing questions and strong interview/permission phrases are handoffs — mid-text `?` and optional-offer closers continue. Policy block / denied-by-policy phrases stay terminal.

3. **Budget + probe.** Default `[agent.harness.continuation].cross_turn_turns` is **32** with **progress-reset**: any newly completed tracker step resets the episode (`SessionStats::note_tracker_completed_count`). Live probes return `TrackerProbeOutcome::{Incomplete,Complete,Unavailable}`; **Complete clears** the incomplete cache so auto-continue stops after the tracker finishes; Unavailable keeps the last known incomplete set.

4. **Planning + resume + UX.** Planning resume auto-queues plan continuation when the blocked-handoff summary is plan-recoverable and no plan is approval-ready (never auto-approves). Successful auto-queue paths print continuation Info lines — never “Type `continue`”. Compiled runtime guidance forbids status-only / “next step on resume” recaps while tracker steps remain.

**Verification** — commands and results:

- `cargo nextest run -p vtcode-config -E 'test(tracker) or test(continuation)'` — PASS (3)
- `cargo nextest run -p vtcode-core -E 'test(runtime_guidance) or test(tracker_final_text) or test(tracker_status)'` — PASS (6)
- `cargo nextest run -p vtcode -E 'test(tracker_probe) or test(session_stats_apply) or test(session_stats_progress) or test(parse_incomplete) or test(recoverable_block) or test(plan_mode_recoverable) or test(tracker_incomplete) or test(tracker_config) or test(outer_queue) or test(resume_gate) or test(parse_tracker)'` — PASS (14)
- `./scripts/check-dev.sh` — PASS
- PRE-EXISTING (unrelated): `sparse_approved_plan_is_distilled_into_tracker_items` may fail; not part of this diff

**Journey log** —

1. Empirical check of the cited session showed auto-continue already worked (20/20 turns, 0 user `continue` prompts); residual UX/classifier/budget gaps were the real bug class.
2. First review (general-1): critical B1 — incomplete cache never cleared on successful complete probes, so auto-continue could fire after the tracker finished.
3. Fix: `TrackerProbeOutcome` + `apply_tracker_probe_to_cache` (Complete = clear, Unavailable = keep last); re-review (general-2) confirmed B1 fixed with no remaining criticals.
4. Drive-by clippy fix in `vtcode-commons::diff_review_notice` (identical if-blocks) cleared the check-dev gate; behavior unchanged.
5. Dual incomplete caches (HarnessTurnState + SessionStats) share the same clear/keep contract; mid-session Unavailable is intentionally fail-open, resume remains fail-closed via `incomplete_tracker_items`.

## [S1] Problem

TODO/tracker work does not continue *seamlessly* from the user’s point of view, and several harness paths still nudge:

1. **Model status recaps still read as handoffs.** Assistant finals like `## Status` / “blocked by turn tool budget” / “Next step on resume” end turns even when `task_tracker` is incomplete.
2. **Planning recovery still nudges.** `current_blocked.md` / footer still say “Type `continue`” / “Type `keep planning`” when planning ends via recovery fallback (`PLANNING_COMPLETED_TURN_FALLBACK_REASON` contains both deny tokens *and* “recovery fallback”; deny-list ran first).
3. **Cross-turn tracker budget is an episode that only resets when the whole tracker completes.** Default `cross_turn_turns = 8` exhausts on long TODO lists, then the outer loop prints “Type `continue` to resume.”
4. **Recoverable classifiers miss production blocked-reasons.** Tool-call / tool-loop budget, tool follow-up recovery, completed-turn-no-response, and plan recovery-exhausted constants were not allow-listed.
5. **In-turn override is skipped or over-denied.** Tool-free recovery status text never got the tracker override; any mid-text `?` ended the turn; `cross_turn_turns == 0` also killed in-turn continuation; live tracker probe failure yielded no queue / stale cache after completion.
6. **UX still printed “Type continue” after a successful auto-queue**, and safety-handoff vocabulary over-matched (`"tool policy"`).

## [S2] Design

User-confirmed decisions (2026-09-19):

- **Full residual hardening** — classifiers, planning path, budget, in-turn override, and user-facing nudge text.
- **Progress-reset + raise default** — any completed tracker step resets the cross-turn auto-continue episode; default `cross_turn_turns` becomes **32**.
- **True handoffs only** — when tracker work remains, stop for the user only on genuine question/decision, permission/policy/safety fuse, missing credentials, verification escalation after recovery is exhausted, or hard session exit. Budget / tool-loop / preview / recovery ends auto-continue.

### A. Recoverable classifiers (shared vocabulary)

**In-turn** (`continuation.rs::apply_tracker_continuation_override`):

- Recoverable budget phrasing includes `"tool budget"`, `"tool loop"`, `"read cap"`, `"work budget"`, `"max tool"`, `"per-turn tool"`, `"tool-call budget"`, `"tool follow-up"`, `"recovery exhausted"`, `"budget exhausted"`.
- When tracker is incomplete, recoverable budget phrasing **wins** even if the text also contains `"blocked by"`.
- **True-handoff filter when tracker incomplete:** trailing clarifying question or strong interview/permission phrases end the turn. Mid-text `?` and optional-offer closers are not handoffs.
- Explicit safety/policy/credentials phrases (including `policy block`, `blocked by policy`, `denied by policy`) always end the turn.

**Outer Completed/Blocked** (`helpers.rs::tracker_auto_continue_is_recoverable_block`):

- Allow-list includes tool-loop/tool-call budget, tool follow-up, recovery exhausted, harness-visible final response miss, per-turn tool limit, read cap, budget exhausted.
- Deny-list keeps verification-pending, context-capacity, contract-violation, unmatched tool result, permission/user-input/safety fuse, interview/approval handoffs.

**Safety-handoff** (`completion.rs::tracker_final_text_is_safety_handoff`):

- Expanded `budget_like` with tool-budget/loop/read-cap tokens.
- Narrowed `"tool policy"` to explicit denial shapes.

**Plan-mode** (`helpers.rs::plan_mode_recoverable_block`):

- Allow-list first for production recovery constants, especially `PLANNING_COMPLETED_TURN_FALLBACK_REASON`.
- Interview/approval/permission handoffs denied after allow-list miss.
- Completed planning turns are never auto-continued. Never auto-approves.

### B. In-turn override wiring

- Applies on **tool-free recovery** status text when tracker is incomplete.
- Gated on `auto_continue_tracker` only.
- `TrackerProbeOutcome` live probe: Incomplete replaces cache, **Complete clears**, Unavailable keeps last.

### C. Outer queue + budget

```toml
[agent.harness.continuation]
auto_continue_tracker = true   # unchanged default
cross_turn_turns = 32          # default (was 8); progress-resets on tracker step completion
```

- Progress-reset via `SessionStats::note_tracker_completed_count` + `reset_tracker_continuation_budget`.
- Queue-first then record budget. Queue-full / truly exhausted budget may mention `continue` only when nothing was queued.
- True-handoff stop list as in S2 decisions.

### D. Planning path + resume

- Recoverable blocked planning ends auto-queue `plan_mode_continue_follow_up()`.
- Resume: plan-recoverable blocked handoff + planning active + no approval-ready plan → auto-queue plan continuation.
- Tracker resume path unchanged aside from classifier/budget/probe fixes.

### E. UX + shipped guidance

- Successful auto-queue: Info lines only — never “Type `continue`”.
- Compiled `runtime_guidance.rs` forbids status-only recaps / “next step on resume” while tracker steps remain.
- Docs: `docs/guides/agent-loop-contract.md`, `docs/config/CONFIG_FIELD_REFERENCE.md`.

### True-handoff stop list (normative)

End turn and wait for the user when tracker work remains **only if**:

1. Text asks a genuine user question/decision (trailing/interview), or
2. Permission / policy / safety fuse / missing credentials handoff, or
3. Verification block after autonomous recovery budget is exhausted / escalated, or
4. Session exit / hard interrupt.

## [S3] Out of Scope

- Auto-approving plans, tools, or permissions.
- Changing anti-blind verification semantics beyond not nudging when recovery was already queued.
- Infinite auto-run without hard session/turn caps or Esc.
- Completing the blocked compose-next checklist for the diff-harness feature (separate work).
- Uncommitted main-tree TUI/diff files not owned by this branch.
- New public APIs outside the harness continuation path.

## Tasks

- [x] T1: Expand recoverable classifiers (in-turn phrasing, outer allow-list, safety-handoff `budget_like` + narrow `"tool policy"`, plan-mode allow-list-first for recovery fallback) — acceptance: unit tests for production constants including planning fallback, tool-loop/tool-call budget, and deny cases still denied (covers: S2A)
- [x] T2: In-turn override — tool-free recovery path, `auto_continue_tracker`-only gate, trailing-question handoff filter while tracker incomplete, probe/cached incomplete-items fallback — acceptance: pure tests for each gate; Complete probe clears cache (covers: S2B; depends: T1)
- [x] T3: Progress-resetting cross-turn budget + default `cross_turn_turns=32` — acceptance: config default test; `SessionStats` resets episode when completed tracker count increases; queue-full still falls through to handoff text only when nothing queued (covers: S2C; depends: T1)
- [x] T4: Planning recoverable-block + resume auto-queue; suppress “Type continue” whenever a continuation turn was queued — acceptance: planning fallback constant auto-queues; resume plan path present; no nudge string on successful queue paths (covers: S2D,S2E; depends: T1)
- [x] T5: Runtime guidance + docs (agent-loop-contract, CONFIG_FIELD_REFERENCE) — acceptance: presence test for strengthened guidance; docs mention default 32, progress-reset, true-handoff stop list (covers: S2E; depends: T3)
- [x] T6: Integration/pure gate regression suite + `./scripts/check-dev.sh` — acceptance: targeted nextest filters pass; check-dev PASS on this worktree; review critical B1 fixed and re-verified (covers: S2; depends: T1–T5)
