---
feature: todo-continuation-hardening
status: in-progress
updated: 2026-09-19
branch: fix/todo-continuation-gaps
commits: # filled at delivery
---

# TODO Continuation Hardening (Residual)

Residual work after delivered `tracker-continuation`. Evidence session `session-vtcode-20260918T030054Z_141498-26410` auto-continued 20 turns with zero user `continue` prompts, but the harness and model still present “stopped / take action” to the user, and several production blocked-reasons still fall through to `Type continue`.

## Report

(Empty at design time.)

## [S1] Problem

TODO/tracker work does not continue *seamlessly* from the user’s point of view, and several harness paths still nudge:

1. **Model status recaps still read as handoffs.** Assistant finals like `## Status` / “blocked by turn tool budget” / “Next step on resume” end turns even when `task_tracker` is incomplete.
2. **Planning recovery still nudges.** `current_blocked.md` / footer still say “Type `continue`” / “Type `keep planning`” when planning ends via recovery fallback (`PLANNING_COMPLETED_TURN_FALLBACK_REASON` contains both deny tokens *and* “recovery fallback”; deny-list runs first).
3. **Cross-turn tracker budget is an episode that only resets when the whole tracker completes.** Default `cross_turn_turns = 8` exhausts on long TODO lists, then the outer loop prints “Type `continue` to resume.”
4. **Recoverable classifiers miss production blocked-reasons.** Tool-call / tool-loop budget, tool follow-up recovery, completed-turn-no-response, and plan recovery-exhausted constants are not allow-listed, so outer auto-queue never fires.
5. **In-turn override is skipped or over-denied.** Tool-free recovery status text never gets the tracker override; any mid-text `?` or “happy to” closer ends the turn; `cross_turn_turns == 0` also kills in-turn continuation; live tracker probe failure yields no queue.
6. **UX still prints “Type continue” after a successful auto-queue**, and safety-handoff vocabulary over-matches (`"tool policy"`).

## [S2] Design

User-confirmed decisions (2026-09-19):

- **Full residual hardening** — classifiers, planning path, budget, in-turn override, and user-facing nudge text.
- **Progress-reset + raise default** — any completed tracker step resets the cross-turn auto-continue episode; default `cross_turn_turns` becomes **32**.
- **True handoffs only** — when tracker work remains, stop for the user only on genuine question/decision, permission/policy/safety fuse, missing credentials, verification escalation after recovery is exhausted, or hard session exit. Budget / tool-loop / preview / recovery ends auto-continue.

### A. Recoverable classifiers (shared vocabulary)

**In-turn** (`continuation.rs::apply_tracker_continuation_override`):

- Treat as recoverable budget phrasing (non-exhaustive): existing tokens plus `"tool budget"`, `"tool loop"`, `"read cap"`, `"work budget"`, `"max tool"`, `"per-turn tool"`, `"tool-call budget"`, `"tool follow-up"`, `"recovery exhausted"`.
- When tracker is incomplete, recoverable budget phrasing **wins** even if the text also contains `"blocked by"` / other `has_explicit_blocker` tokens.
- **True-handoff filter when tracker incomplete:** only a *trailing* clarifying question or interview phrases (`?` at the end / closing ask) end the turn. Mid-text `?` in status sections and optional-offer closers (`happy to`, `let me know if`, `if you want me to`) are **not** handoffs while incomplete tracker work remains.
- Override remains skipped for empty text, planning active, and genuine permission/policy/safety/credentials handoffs.

**Outer Completed/Blocked** (`helpers.rs::tracker_auto_continue_is_recoverable_block`):

- Allow-list additional production shapes: `"tool loop budget"`, `"tool-call budget"`, `"tool follow-up"`, `"recovery exhausted"`, `"without a harness-visible final assistant response"`, `"max tool"`, `"per-turn tool limit"`.
- Keep deny-list for verification-pending, context-capacity, contract-violation, unmatched tool result, permission/user-input/safety fuse, interview/approval handoffs.

**Safety-handoff** (`completion.rs::tracker_final_text_is_safety_handoff`):

- Expand `budget_like` with the same tool-budget / tool-loop / read-cap tokens as the in-turn classifier.
- Narrow `"tool policy"` over-match to explicit denial shapes: `"denied by tool policy"`, `"denied by workspace tool policy"`, `"blocked by policy"`, `"execution denied by policy"`, `"policy block"`.

**Plan-mode** (`helpers.rs::plan_mode_recoverable_block`):

- Evaluate the **allow-list first** for production recovery constants, especially `PLANNING_COMPLETED_TURN_FALLBACK_REASON` (“Planning turn ended via recovery fallback … approval-ready plan …”). Matching `"recovery fallback"` makes the reason recoverable.
- Deny-list applies only when the allow-list does not match. Narrow deny tokens so they do not shadow the fallback constant: interview/approval waits (`"approval-ready plan remains"`, `"planning interview"`, `"awaiting approval"`, `request_user_input`, permission/policy) stay denied.
- Completed planning turns remain **never** auto-continued (interview/approval risk). Never auto-approves.

### B. In-turn override wiring

- Apply the tracker override on **tool-free recovery** status text when tracker is incomplete (tools are already disabled; a status recap must not strand the session).
- Gate in-turn override on `auto_continue_tracker` only — **do not** require `cross_turn_turns > 0` (that knob disables only cross-turn queue).
- Cache last-known incomplete tracker items (session-scoped, updated whenever `incomplete_tracker_items` succeeds). Outer/in-turn gates fall back to the cache when the live probe fails, so a transient tracker read error does not drop auto-queue.

### C. Outer queue + budget

```toml
[agent.harness.continuation]
auto_continue_tracker = true   # unchanged default
cross_turn_turns = 32          # NEW default (was 8)
```

- **Progress-reset:** `record_tracker_continuation_turn_*` / outer gate treat a completed tracker step since the episode started as a reset of `tracker_continuation_turns` to 0 before queueing. Implementation: track `last_seen_completed_tracker_count` in `SessionStats`; when live completed-count increases, call `reset_tracker_continuation_budget()`.
- Queue-first then record budget (unchanged). Queue-full / truly exhausted budget may still mention `continue`, but only when no continuation turn was queued.
- **True handoffs only** for stop conditions listed in S2 decisions. `should_queue_tracker_auto_continue` keeps: kill-switch, planning_active (tracker path), empty incomplete, safety-handoff, verification-block (separate recovery path), non-recoverable blocked reason, `cross_turn_turns == 0`.
- No new config keys beyond the default change for `cross_turn_turns`.

### D. Planning path + resume

- After A’s plan-mode classifier fix, recoverable blocked planning ends auto-queue `plan_mode_continue_follow_up()` via existing `should_queue_plan_mode_auto_continue`.
- **Resume:** when a session restores with a live blocked-handoff reason that `plan_mode_recoverable_block` accepts and planning is still active with no approval-ready plan, auto-queue plan continuation (in addition to existing tracker resume auto-queue).
- Resume tracker path unchanged aside from classifier/budget fixes.

### E. UX + shipped guidance

- When tracker or plan auto-queue **succeeds**, render an Info line that work is continuing (`[i] Tracker auto-continue turn N/M…` / plan equivalent). **Never** print “Type `continue`” on a path that queued.
- “Type `continue`” remains only for true handoffs / queue-full / budget exhausted after progress-reset rules.
- Compiled `runtime_guidance.rs`: strengthen the existing tracker line so status-only recaps and “next step on resume” language are forbidden while `task_tracker` steps remain; keep presence/budget test in sync.
- Docs: `docs/guides/agent-loop-contract.md`, `docs/config/CONFIG_FIELD_REFERENCE.md` (default 32 + progress-reset + true-handoff stop list).

### True-handoff stop list (normative)

End turn and wait for the user when tracker work remains **only if**:

1. Text asks a genuine user question/decision (trailing/interview), or
2. Permission / policy / safety fuse / missing credentials handoff, or
3. Verification block after autonomous recovery budget is exhausted / escalated, or
4. Session exit / hard interrupt.

Everything else with incomplete tracker steps continues (in-turn and/or outer queue).

## [S3] Out of Scope

- Auto-approving plans, tools, or permissions.
- Changing anti-blind verification semantics beyond not nudging when recovery was already queued.
- Infinite auto-run without hard session/turn caps or Esc.
- Completing the blocked compose-next checklist for the diff-harness feature (separate work).
- Uncommitted main-tree TUI/diff files not owned by this branch.
- New public APIs outside the harness continuation path.

## Tasks

- [ ] T1: Expand recoverable classifiers (in-turn phrasing, outer allow-list, safety-handoff `budget_like` + narrow `"tool policy"`, plan-mode allow-list-first for recovery fallback) — acceptance: unit tests for production constants including planning fallback, tool-loop/tool-call budget, and deny cases still denied (covers: S2A)
- [ ] T2: In-turn override — tool-free recovery path, `auto_continue_tracker`-only gate, trailing-question handoff filter while tracker incomplete, cached incomplete-items fallback — acceptance: pure tests for each gate; override applies on tool-free status text (covers: S2B; depends: T1)
- [ ] T3: Progress-resetting cross-turn budget + default `cross_turn_turns=32` — acceptance: config default test; `SessionStats` resets episode when completed tracker count increases; queue-full still falls through to handoff text only when nothing queued (covers: S2C; depends: T1)
- [ ] T4: Planning recoverable-block + resume auto-queue; suppress “Type continue” whenever a continuation turn was queued — acceptance: planning fallback constant auto-queues; resume plan path test; no nudge string on successful queue paths (covers: S2D,S2E; depends: T1)
- [ ] T5: Runtime guidance + docs (agent-loop-contract, CONFIG_FIELD_REFERENCE) — acceptance: presence test for strengthened guidance; docs mention default 32, progress-reset, true-handoff stop list (covers: S2E; depends: T3)
- [ ] T6: Integration/pure gate regression suite + `./scripts/check-dev.sh` — acceptance: targeted nextest filters pass; check-dev PASS on this worktree (covers: S2; depends: T1–T5)
