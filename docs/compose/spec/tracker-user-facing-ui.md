---
feature: tracker-user-facing-ui
status: in-progress
updated: 2026-09-18
branch: feat/tracker-user-facing-ui
commits: # filled at delivery
---

# Tracker User-Facing UI (Title + Progress Only)

## Report

## [S1] Problem

The product goal for user-facing task tracking is: show only the relevant task **title and progress**, hide internal tracking details, and keep the internal tracker robust for the agent and session resume.

Today every successful `task_tracker` update paints the **full compact tree** into both the transcript (`• Tasks 1/4 — next: …` plus every step row) and the docked task-panel body. Operational step lists, status glyphs, and next-step snippets are harness detail; the user surface should answer “what is this work, and how far along is it?” without dumping the checklist on every update.

Internal tracking is already complete: `[agent.harness.continuation]` auto-continues incomplete tracker work across turns/resume, and the tool payload still carries full checklist items plus structured `files`/`outcome`/`verify` metadata. The gap is presentation only.

## [S2] Design

Decision (user-confirmed):

- **User-facing surface = title + progress only.**
- Transcript tracker updates are a single progress line.
- Docked task panel (Alt+G) keeps the compact tree in its **body when the user opens it**; header already shows title + progress.
- Internal `task_tracker` payload and continuation behavior stay unchanged.

### Contracts

**View split** in `src/agent/runloop/tool_output/mod.rs` (binary presentation layer):

```rust
/// Transcript-facing user surface: title + progress only (plus diagnostics).
pub(crate) fn tracker_progress_lines(val: &serde_json::Value) -> Vec<String>

/// Panel body: compact tree rows only (no summary header / no next: snippet).
pub(crate) fn tracker_tree_body_lines(val: &serde_json::Value) -> Vec<String>
```

- `tracker_progress_lines` success shape (one line):
  - `• {humanized_title} {completed}/{total}` when a non-empty checklist title exists
  - `• Tasks {completed}/{total}` otherwise
  - **No** `— next:` snippet, **no** tree rows
- `tracker_progress_lines` failure/empty/malformed: keep existing diagnostic lines (`Tracker status: …`, `Update: …`) so errors stay visible
- `tracker_tree_body_lines`: `compact_task_tree_view_from_items` / visible view rows only; strip `files:` / `outcome:` / `verify:` as today; do not prepend the summary header
- `tracker_panel_metadata(val)`: unchanged (`title` humanized, `completed`, `total`)
- Structured tool payload for the model is **not** narrowed

**Call-site wiring**

1. `src/agent/runloop/unified/tool_output_handler.rs` (`is_task_tracker_tool` path):
   - Panel: `update_task_panel_with_metadata(tracker_tree_body_lines(output), tracker_panel_metadata(output))`
   - Transcript: `apply_task_tracker_block(..., tracker_progress_lines(output))`
2. `src/agent/runloop/unified/planning_workflow/task_tracker.rs` (`render_created_task_tracker`):
   - Same split; approval handoff may still `show_task_panel()` once
   - Transcript append / replace uses progress-only lines (dedupe on those lines)
3. Terminal title (`vtcode-ui` session):
   - When task-panel metadata updates, set `terminal_title_task_progress` from `format!("{completed}/{total}")`
   - Stop depending on parsing a `Progress: ` row out of panel body lines for this path

**Panel UI** (`vtcode-ui` task panel — unchanged layout rules)

- Header: humanized title + `{completed}/{total} complete` (already)
- Body: compact tree when panel is visible; empty-state constant when no rows
- Appearance-suppressed panel still retains lines for later manual toggle

**Prompt / docs surface**

- Update the binary gotcha that currently requires transcript + panel to share `tracker_view_lines` as one compact tree: transcript uses progress-only; panel body uses the tree
- No change to continuation guidance or `task_tracker` tool schema docs beyond presentation notes if they claim the transcript shows the full tree

### Config

No new config keys. Presentation is unconditional under the existing tracker UI; kill-switches for continuation remain `[agent.harness.continuation]`.

## [S3] Out of Scope

- Changing `task_tracker` tool schema, persistence, or model-facing payload shape
- Changing continuation / resume auto-queue behavior (`tracker-continuation` stays as delivered)
- Auto-opening the task panel on every tracker update
- Redesigning status glyphs or panel chrome beyond the progress/title vs tree split
- Fixing unrelated TODO.md owner notes

## Tasks

- [ ] T1: Add `tracker_progress_lines` + `tracker_tree_body_lines` with unit tests — acceptance: success emits one title+progress line (no next/tree); errors keep diagnostics; tree body has no summary header and no metadata rows (covers: S2)
- [ ] T2: Wire tool_output_handler + planning approval handoff to the view split — acceptance: transcript replace uses progress-only; panel receives tree body + metadata; existing dedupe still holds (covers: S2; depends: T1)
- [ ] T3: Terminal title progress from panel metadata — acceptance: metadata update sets `N/M` progress for terminal title without parsing tree lines (covers: S2; depends: T2)
- [ ] T4: Update transcript/panel tests that assert full-tree user-facing blocks — acceptance: tests assert progress-only transcript rows and tree rows only on the panel path (covers: S2; depends: T2)
- [ ] T5: Update AGENTS/gotcha/docs presentation contract — acceptance: binary gotcha no longer mandates shared full-tree transcript rendering; docs describe title+progress transcript vs panel tree (covers: S2; depends: T2)
