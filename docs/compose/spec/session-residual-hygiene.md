---
feature: session-residual-hygiene
status: in-progress
updated: 2026-09-26
branch: fix/session-residual-hygiene
commits: # leave empty while in progress
---

# Session Residual Hygiene

## Report

## [S1] Problem

Re-audit of `session-vtcode-20260925T234343Z_201620-81429` after the delivered
`session-audit-harness-fixes` pass. That pass fixed preflight-circuit sibling
skips, approval-cache growth, and spool leftovers. Residual state and context
contamination still let one session affect later ones and still grow `.vtcode`
without bound.

Evidence on the live workspace after the prior pass:

1. **Completed sessions pin turn checkpoints for the full 30-day window.**
   `.vtcode/checkpoints/` holds 125 `turn_*.json` snapshots (~59 MB) and 14
   `branch_*.json` navigation files; the directory is 185 MB total.
   `protected_turns_with_cutoff` only ignores a branch file after
   `max_age_days` (30). The audited session's checkpoint still has
   `active: [1435..1450]` (16 turns) even though the thread ended
   `cancelled`/`exit`. `prune_snapshot_budget` never evicts protected turns, so
   a handful of recent sessions pin most of the directory.

2. **Global task artifacts leak into later sessions' memory envelopes.**
   `.vtcode/tasks/current_spec.md` is a Jul 24 fixture
   (`# Execution Spec` / `Explore the codebase and summarize.`). The audited
   session's `history/*.memory.json` recorded
   `spec_summary: "Spec: Explore the codebase and summarize."` from that file,
   not from the session's real objective ("Fix review finding 1").
   `current_contract.md` and `current_feature_list.md` are the same stale
   fixture family. `current_task.md` still holds the finished checklist.

3. **Malformed `verify:` lines leak into `verification_summary`.** Plan step 5
   of the audited work wrote
   `verify: [cargo nextest run -E '...'] and verify: [cargo check --locked`
   on one line. `collect_structured_verify_commands` takes the whole tail of
   `verify:` as one command, so the history envelope's `verification_summary`
   contains the literal `'] and verify: ['` splice.

4. **Session retention skips abandoned `active` stores.**
   `vtcode_memory::apply_retention_preserving` (default 50 sessions / 30 days)
   is wired at harness finalization, but `retention_candidates` skips every
   store whose manifest `status == "active"`. On disk: 167 manifests, 111 still
   `active`, 35 of those older than 7 days, oldest 43 days. Crashed/killed
   threads never see `thread.completed`, so they stay `active` forever and
   retention never removes them. 5 sessions are retention-pinned.

5. **`history/` envelopes have no retention.** Memory envelopes are still
   written to `.vtcode/history/*.memory.json` (1005 files). `gc_legacy` can
   delete the directory after migration, but interactive sessions keep writing
   envelopes there and nothing caps the count.

6. **Stale rewind lock.** `checkpoints/rewind.lock` is a zero-byte file dated
   2026-09-24 and is never cleared on thread completion.

## [S2] Design

### A. Stop completed threads from pinning their whole turn history

On `thread.completed` (any outcome), rewrite the session's `branch_*.json`
navigation record so only a **bounded rewind window** of turns stays protected:

- Keep `active` as the last `REWIND_ACTIVE_KEEP` (5) turn numbers, plus any
  `pending` recovery, plus `redo` (already cleared on completion).
- Drop older `active` entries from the branch file. Their `turn_*.json`
  snapshots become eligible for `prune_snapshot_budget` /
  `cleanup_old_snapshots`.
- The existing age-cutoff rule in `protected_turns_with_cutoff` stays as the
  backstop for branches that never see a completion event.

Acceptance shape: after a thread completes with `active: [1..20]`, the branch
file lists at most 5 active turns (the highest numbers) and the other 15
snapshots are unprotected.

### B. Session-scoped context artifacts

Stop later sessions from inheriting stale global task/spec files:

1. When building a memory envelope / orient snapshot, drop
   `spec_summary`/`contract_summary`/`feature_list_summary` whose source file
   `mtime` is older than the current session's `created_at`. A leftover fixture
   must not describe a new session.
2. On `thread.completed` with a terminal outcome (`completed`, `cancelled`,
   `exit`): if `current_task.md` exists and every checklist item is `- [x]`,
   move it aside to `.vtcode/tasks/archive/current_task-<session_id>.md` (keep
   the live path clear for the next plan). Do **not** delete uncompleted
   checklists.
3. Delete the known-stale Jul 24 fixture trio live (operational, not in the
   diff): `current_spec.md`, `current_contract.md`, `current_feature_list.md`
   when their content is exactly the fixture text.

### C. Parse `verify:` lines without splicing

`collect_structured_verify_commands` must:

- Strip surrounding `[` `]` from a command.
- Split a single `verify:` line on ` and verify:` so
  `verify: [a] and verify: [b]` yields two commands.
- Drop empty commands after strip.

The plan tracker already accepts repeated `verify:` lines; the fix is
downstream of that so both shapes land in `verification_summary` as separate
bullets.

### D. Retention: abandoned `active` sessions and `history/` envelopes

1. **Abandoned active stores become eligible.** In
   `retention_candidates`, treat `status == "active"` as evictable when
   `updated_at` is older than `RetentionPolicy::max_age_days` (default 30) —
   the same age budget already applied to completed sessions. Still skip a
   preserved session id and retention-pinned stores. A live session is younger
   than the cutoff, so it remains safe.
2. **`history/` envelope cap.** After session finalization (same
   `spawn_blocking` site that already calls `apply_retention_preserving`), prune
   `.vtcode/history/*.memory.json`:
   - Keep the most recent `HISTORY_ENVELOPE_KEEP` (50) files by mtime.
   - Delete envelopes older than `max_age_days`.
   - Never delete the envelope belonging to the session being finalized.
3. Existing `RetentionPolicy::default()` (50 sessions / 30 days) and
   `apply_retention_preserving` keep their contract.

### E. Clear stale `rewind.lock`

On `thread.completed`, remove `checkpoints/rewind.lock` when this session owns
it (best-effort; missing file is success). Live cleanup also deletes the
current zero-byte Sep 24 lock.

## [S3] Out of Scope

- Already delivered by `session-audit-harness-fixes`: preflight LLM-mistake
  classification, approval-cache budget, spool startup prune, quoted-heredoc
  injection false positives.
- Unifying `history/` envelopes into the per-session `derived/memory.json`
  store (path migration). This pass only caps the legacy directory.
- Model-side thrashing (repeated identical test invocations).
- Changing `max_snapshots` (50) or `DEFAULT_MAX_AGE_DAYS` (30) defaults.
- `/revert` UX and checkpoint format changes beyond the branch-file `active`
  trim.
- TODO.md (owner-only).

## Tasks
- [x] T1: Trim completed threads' branch `active` list to a bounded rewind window on `thread.completed` — acceptance: unit test writes a branch file with 20 active turns, runs the completion trim, asserts ≤5 remain and the dropped turn ids are unprotected (covers: S2.A)
- [x] T2: Drop stale global task/spec summaries older than session start, and archive a fully-checked `current_task.md` on terminal completion — acceptance: envelope built with an older-than-session `current_spec.md` has `spec_summary: None`; fully-checked tracker is archived and live path cleared (covers: S2.B)
- [x] T3: Split/strip `verify:` command collection so bracketed multi-commands do not splice — acceptance: `verify: [cargo nextest run -E 'x'] and verify: [cargo check --locked` yields two clean commands in `verification_summary` (covers: S2.C)
- [x] T4: Evict abandoned `active` session stores past `max_age_days`, and cap `history/*.memory.json` at 50 newest / age cutoff at finalization — acceptance: retention removes an `active` store whose `updated_at` is older than the cutoff and skips a young `active` one; history prune keeps the current session's envelope (covers: S2.D)
- [x] T5: Remove stale `rewind.lock` on terminal completion — acceptance: completion of a session that created the lock deletes it (covers: S2.E)
- [x] T6: Live residual cleanup on the workspace and record results in Report — acceptance: checkpoints pruned to budget, fixtures gone, rewind.lock gone, history capped, Report filled (covers: S1)
