---
feature: planning-verify-commands
status: designed
updated: 2026-09-19
branch: fix/planning-verify-commands
commits:
---

# Planning verify commands accept common inspection tools

## Report

## [S1] Problem

Planning recovery and plan repair reject valid inspection `verify:` items.

Evidence:

- Live blocker `.vtcode/tasks/current_blocked.md` for
  `session-vtcode-20260918T101837Z_175560-33614`: planning ended via recovery
  fallback without an approval-ready plan (324k prompt tokens, tool preview
  budget exhaustion class of failure).
- Checkpoints `turn_1234`–`turn_1243` show the same validation error on
  README/docs plans:
  `invalid plan artifact: invalid implementation steps: … verification item 1
  must be a concrete command or check`.
- Rejected verify values in those drafts were ordinary shell inspection
  commands the planning workflow itself uses for research, for example:
  - `verify: [sed -n '81,88p' README.md]`
  - `verify: [grep -n 'planning-workflow' README.md]`
  - `verify: [sed -n '/## Why VT Code/,/## Architecture/p' README.md]`
- Root cause is in shipped validator `COMMAND_NAMES` in
  `crates/codegen/vtcode-core/src/tools/handlers/planning_workflow/artifacts.rs`:
  the allowlist includes `cargo`/`rg`/`python3`/… but omits `sed`, `grep`,
  `head`, `tail`, `wc`, `cat`, `find`, and similar inspection tools.
  `validate_concrete_verification` therefore reports them as
  `verification item N must be a concrete command or check`.
- Prompt surfaces reinforce the gap: `PLANNING_SYNTHESIS_FORMAT_HINT`,
  `PlanValidationReport::repair_feedback`, and
  `PLANNING_WORKFLOW_PLAN_QUALITY_LINE` list only `cargo`/`rg` as valid
  examples, so recovery synthesis keeps emitting `sed`/`grep` and bounded
  repairs fail until the model luckily switches tools.

User-visible impact: planning research that already gathered evidence cannot
close a plan; recovery synthesizes a draft, validation rejects concrete
inspection commands, repair budget burns out, and the turn ends blocked.

## [S2] Design

Treat common POSIX inspection commands as concrete verification tokens when
they appear as the command head of a `verify:`/`verification:` item.

### Validator contract (`artifacts.rs`)

1. Expand `COMMAND_NAMES` with inspection/verification binaries that planning
   legitimately uses, at minimum:
   `sed`, `grep`, `egrep`, `fgrep`, `head`, `tail`, `wc`, `cat`, `find`, `ls`,
   `awk`, `sort`, `uniq`, `cut`, `tr`, `diff`, `jq`, `stat`, `file`.
2. Keep existing safety rules intact:
   - path-like tokens still require safe workspace-relative shape
     (`is_safe_workspace_relative_command_token`); no URL schemes, no shell
     metacharacters as path components.
   - `git diff --check` remains invalid (VCS-only check is explicitly listed
     as invalid in current guidance; do not add bare `git` as a free pass for
     that pattern).
   - observational verification and independent-rederivation paths stay
     unchanged.
3. Quoted-comma `sed`/`grep` ranges must continue to parse as one bracket item
   (`split_bracket_items`); no regression to the 2026-09-14 fix.

### Prompt contract (compiled / runloop-owned surfaces)

Synchronize valid examples so every prompt surface the model sees includes at
least one inspection-command example next to `cargo`/`rg`:

- `src/agent/runloop/unified/turn/guards.rs` — `PLANNING_SYNTHESIS_FORMAT_HINT`
- `crates/codegen/vtcode-core/src/tools/handlers/planning_workflow/artifacts.rs`
  — `PlanValidationReport::repair_feedback`
- `crates/codegen/vtcode-core/src/prompts/system.rs` —
  `PLANNING_WORKFLOW_PLAN_QUALITY_LINE` (embed `CANONICAL_STEP_FORMAT`; keep
  the existing prompt sync test green)

Valid examples must include something equivalent to:
`verify: [sed -n '1,40p' path/file.md]` and/or
`verify: [grep -n 'symbol' path/file.rs]`.
Invalid examples stay `verify: [run checks]`, `verify: [check later]`,
`verify: [git diff --check]`.

### Recovery path

No change to recovery control flow: tool-free recovery already allows bounded
validation repair (`response_handling.rs` `reject_plan_artifact` tool-free
branch). The fix is that recovery drafts using inspection commands validate on
first synthesis or first repair instead of exhausting the repair budget.

## [S3] Out of Scope

- Changing planning recovery thresholds, preview budgets, or blocked-handoff
  policy.
- Adding a free-form “any token that looks like a shell command” heuristic.
- Accepting `git diff --check` / generic VCS-only verification.
- Changing `ThreadEvent`, plan persistence layout, or tracker generation
  semantics beyond validation outcomes for previously rejected inspection
  verifies.
- Unrelated uncommitted work on `main` (error.rs / retry.rs / commons /
  TODO.md) — not part of this feature branch.

## Tasks

- [ ] T1: Expand inspection command allowlist in plan verification — acceptance:
  `validate_plan_content` accepts `verify: [sed -n '81,88p' README.md]`,
  `verify: [grep -n 'planning-workflow' docs/guides/planning-workflow.md]`, and
  a quoted-comma `sed` range without splitting; still rejects
  `verify: [run checks]` and `verify: [git diff --check]`.
  (covers: S2)
- [ ] T2: Sync planning prompt valid examples with inspection commands —
  acceptance: `PLANNING_SYNTHESIS_FORMAT_HINT`, `repair_feedback()`, and
  `PLANNING_WORKFLOW_PLAN_QUALITY_LINE` each include an inspection-command
  valid example; existing CANONICAL_STEP_FORMAT sync test still passes.
  (covers: S2; depends: T1)
- [ ] T3: Regression tests from live rejected verify strings — acceptance:
  focused unit tests in `planning_workflow` cover the checkpoint-1234-style
  `sed`/`grep` verifies, quoted-comma `sed` still one item, and generic prose
  still fails. (covers: S2; depends: T1)
- [ ] T4: Document the planning verify contract — acceptance:
  `docs/harness/SESSION_LOG_REVIEW.md` gains a dated entry for this failure
  class, and planning docs that list verification examples mention inspection
  commands where they currently imply only build/test tools.
  (covers: S2; depends: T1, T2)
