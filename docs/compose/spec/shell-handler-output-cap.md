---
feature: shell-handler-output-cap
status: in-progress
updated: 2026-09-12
branch: fix/tools-shell-handler-output-cap
commits: # filled at delivery
---

# Shell Handler Output Cap

## Report

## [S1] Problem

`ShellHandler` buffers the full stdout/stderr of every shell command and returns it concatenated as model-visible content with no byte or token cap. Unlike the `exec_command` registry path (which applies `max_output_tokens` preview budgets, spooling, and head/tail excerpts), a single `cargo build`, `find /`, or test log can dump multi-megabyte text into the conversation context. This is the largest single-call token waste in the tools surface.

Evidence: `crates/codegen/vtcode-core/src/tools/handlers/shell_handler.rs:113-192` — `command.output()` → `String::from_utf8_lossy` → concatenated `content_text` with zero truncation.

## [S2] Design

Reuse the existing shared budget semantics already used by `exec_command`:

1. Parse an optional `max_output_tokens` integer from the shell tool arguments (same field name and bounds as `vtcode_utility_tool_specs::MAX_OUTPUT_TOKENS_FIELD`).
2. Default to `DEFAULT_MAX_OUTPUT_TOKENS` (10_000) when omitted.
3. Convert to a byte budget via `OUTPUT_PREVIEW_CHARS_PER_TOKEN` (4 chars/token).
4. If the combined stdout+stderr content exceeds the budget, condense with `vtcode_commons::preview::condense_text_bytes` (head + tail) and append a machine-readable truncation marker including original size.
5. Advertise `max_output_tokens` in the `shell` tool schema via `with_max_output_tokens_parameter`.
6. Keep exit-code and stderr-tagging behavior unchanged; only the model-visible preview is bounded.

Contracts:
- Non-oversized output is byte-identical to today.
- Oversized output keeps a head and tail excerpt plus `… [N bytes omitted] …` from `condense_text_bytes`.
- `truncated: true` is indicated in the text marker so the model can re-read via `exec_command` spool or a narrower command.
- `ThreadEvent` contract is untouched; this change only affects the tool result string.

## [S3] Out of Scope

- Wiring the full `exec_command` spooler into `ShellHandler` (larger refactor; preview budget is sufficient).
- Changing `exec_command` defaults or spooling thresholds.
- Safety/gateway changes.

## Tasks

- [x] T1: Add `max_output_tokens` parsing + byte-budget truncation in `ShellHandler::handle` — acceptance: oversized stdout/stderr is condensed; small output unchanged (covers: S2)
- [x] T2: Advertise `max_output_tokens` on the `shell` tool schema — acceptance: schema includes the field with default 10_000 (covers: S2; depends: T1)
- [x] T3: Regression tests for oversized and undersized output — acceptance: tests pass under `cargo nextest run -p vtcode-core -E 'test(shell_handler)'` (covers: S2; depends: T1)
