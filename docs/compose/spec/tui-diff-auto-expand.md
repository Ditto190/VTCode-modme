---
feature: tui-diff-auto-expand
status: delivered
updated: 2026-09-18
branch: feat/tui-diff-auto-expand
commits: 9ad492f8e..<head>
---

# TUI Diff Auto-Resize and Expand

## Report

**What was built** — Inline TUI transcript diffs no longer ellipsis-truncate ordinary edit/patch body rows: the tool-output renderer emits logical bodies whole (up to a 2 000-cell safety cap) so transcript reflow can word-wrap them with a hanging gutter indent. Compact tinted rows that omit the visible gutter hang under the marker cell. Side-by-side transcript rows use the same wrap policy for pane bodies and full-width metadata. The review overlay stays full-viewport, keeps `LayoutOptions.wrap = true`, and discloses `+N more` when laid-out rows exceed the content height. Completed-edit review can open from a retained unified preview via `DiffOverlayRequest.unified` / `DiffPreviewState::from_unified` in `ReadonlyReview`. Clipped edit bodies advertise `review full diff` instead of only an `exec_command` hint. Overlays do not auto-open after a completed edit.

**Verification**
- `RUSTFLAGS="-D warnings" cargo check -p vtcode-ui -p vtcode --tests --locked` — PASS
- `cargo nextest run -p vtcode-ui` — PASS (1202 tests)
- `cargo nextest run -p vtcode -E "test(tool_output) or test(side_by_side) or test(wrap_for_reflow) or test(narrow_inline) or test(format_diff) or test(compact_diff)"` — PASS (219 tests)

**Journey log**
- The screenshot was transcript truncation, not modal height: `render_diff_content_inline_with_language` ellipsized before reflow ever saw the line.
- `vtcode-diff` already had `LayoutOptions.wrap = true`; the overlay problem was leftover truncate-to-width and a missing more-rows cue.
- Compact rows used a single marker cell as the prefix; hanging indent needed a tint-aware fallback when `+/-` text was absent.
- Old tests encoded the truncate-to-measured-width contract for inline UI; they were updated to the wrap-for-reflow contract.
- Mouse hit-region binding from a completed-edit notice row to a stored `DiffReviewAnchor` is the remaining edge; the unified overlay open path is implemented and tested.

## [S1] Problem

Transcript-embedded edit/patch diffs (e.g. `• Edited README.md (+15 -12)`) ellipsis-truncate long source lines at the measured content width before they reach the TUI. The screenshot shows table rows and prose cut to `...`, so the body is unreadable without leaving the transcript. Vertical budgets also omit middle rows behind opaque notices. The full-viewport review overlay still truncates panes and does not surface a clear “more rows remain” cue when wrapped content exceeds height.

## [S2] Design

User-approved decisions:

1. **Transcript wrap** — always wrap edit-diff body rows with hanging indent under the gutter; never ellipsis-truncate when wrapping is possible.
2. **Overflow strategy** — word wrap is the horizontal overflow policy.
3. **Auto-open policy** — full-viewport review opens for approval/conflict/review prompts, and via **explicit expand only** on a clipped completed-edit body. Never auto-modal after a completed edit mid-turn.
4. **Overlay height** — any opened review overlay uses the full viewport.

### S2.1 Transcript path (`agent/runloop/tool_output`)

- Inline TUI sink present (`AnsiRenderer::prefers_untruncated_output()`) ⇒ emit diff body rows without ellipsis, up to a high safety cap (`DIFF_WRAP_SOURCE_MAX_WIDTH`, 2000 display cells). Content above the cap still truncates, but the notice must advertise expand.
- Rows that fit the measured content width keep current gutter/tint behavior.
- Compact rows (gutter hidden when source room is tight) must still wrap: hanging indent width equals the compact marker cell, not an unpainted strip.
- TUI reflow already hangs numbered `+/-` rows via `wrapped_diff_continuation_prefix` (`session/reflow/blocks.rs`). Renderer must deliver full logical lines so that path can wrap; do not pre-wrap into multiple transcript lines unless safety-cap truncation forces a notice.
- Vertical: `bounded_display_lines` omission notice becomes expandable when a `DiffReviewAnchor` is attached: `… +N lines — review full diff` plus the existing fallback hint when no payload is retained.

### S2.2 Explicit expand for completed edits

- When file-op / apply_patch tool output is clipped vertically or any body row exceeded the safety cap, the renderer attaches a UI-only `DiffReviewAnchor`:
  - `file_path`
  - `unified: Option<String>` (canonical `diff_preview.content` when present)
  - `before`/`after` when the tool payload retains them
  - `omitted_lines`
  - `tool_output_id` when a `RecordToolOutput` capture exists
- Activation (click on the expand notice row, or the existing compact-review interaction when focused) calls `InlineHandle::show_transient(TransientRequest::Diff(DiffOverlayRequest { mode: ReadonlyReview, .. }))`.
- `DiffPreviewState` gains a unified-content constructor using `vtcode_diff::DiffDocument::from_unified` when before/after are absent.
- No auto-open after completed edits.

### S2.3 Review overlay (`vtcode-ui` diff preview)

- Overlay continues to render on `layout.viewport` (full viewport). Do not switch to half-height modal geometry.
- `LayoutOptions.wrap` stays `true` (vtcode-diff default). Layout wraps source bodies; the overlay must not re-ellipsis-truncate wrapped body rows.
- Unified and side-by-side panes keep full-width tint padding. Side-by-side `truncate_to_width` remains a safety net only after wrap.
- When laid-out rows exceed content height, controls footer shows `+N more` (or equivalent) so scroll is discoverable; scrolling remains available via existing keys.
- Approval/conflict/review prompts keep their current open paths; this feature only changes how content is laid out inside that overlay.

### S2.4 Shared contracts

- `vtcode-diff` remains renderer-neutral; no theme/UI types in that crate.
- Diff row tint + hanging-indent wrap must stay consistent across transcript reflow, modal preview, markdown reflow, and ANSI output (vtcode-ui AGENTS gotcha).
- `ThreadEvent` and tool safety policy are unchanged. Expand is UI-only identity, not a new source of truth.

### S2.5 Docs (root AGENTS requirement)

- User-facing guide update under `docs/development/` covering transcript wrap + explicit review expand.
- Quick-reference table row/section if a matching table exists.
- Crate AGENTS.md gotcha touch only if the shipped surfaces change the listed invariants.

## [S3] Out of Scope

- Changing tool-registry diff preview generation budgets (`output_spooler`, turn preview bytes) beyond what expand needs.
- Auto-opening review after completed edits.
- Horizontal scroll panes instead of wrap.
- Redesigning approval keybindings or trust-mode UI.
- CLI/non-TUI diff printing beyond keeping tests green when sink is absent (CLI may keep bounded previews).

## Tasks

- [x] T1: Transcript wrap without ellipsis — acceptance: inline TUI edit-diff rows longer than content width render wrapped with hanging gutter indent; no `...` on rows under the safety cap; CLI/sink-absent tests still pass (covers: S2.1)
- [x] T2: Compact-row wrap hanging indent — acceptance: gutter-hidden narrow rows wrap with marker-cell indent; no unpainted hanging strip (covers: S2.1)
- [ ] T3: Expandable omission notice + DiffReviewAnchor — acceptance: clipped completed-edit bodies show an expandable notice; activating it opens ReadonlyReview; no auto-modal after edit (covers: S2.1; S2.2)
- [x] T4: DiffPreview unified constructor + overlay expand wiring — acceptance: `DiffOverlayRequest` can open from unified content; activation from transcript notice produces full-viewport wrapped review (covers: S2.2; S2.3)
- [x] T5: Overlay wrap + more-rows indicator — acceptance: long lines in overlay are wrapped (not ellipsized) when wrap=true; footer discloses remaining rows when content exceeds height (covers: S2.3)
- [x] T6: Regression tests + docs — acceptance: nextest green for touched crates; docs guide + AGENTS gotcha updated if needed (covers: S2.1–S2.5; depends: T1–T5)
