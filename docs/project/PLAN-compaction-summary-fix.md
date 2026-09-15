# PLAN: `/compact` "Failed to generate compaction summary"

Status: **in progress — not green**. The source fix is applied but the unit
test and the local gate do not pass yet (see [Remaining work](#remaining-work)).

Tracks the `.vtcode`-visible symptom:

```
Compaction failed: Failed to generate compaction summary
```

## Root cause

The **local summary path bounded its output but not its input.**

1. `summarize_locally` (`crates/codegen/vtcode-core/src/compaction/mod.rs`)
   builds the summarization request with
   `build_cache_safe_compaction_history(history, instructions)` — the parent's
   **entire** conversation verbatim, plus the compaction instruction.
2. `context_bounded_compaction_config` only shrinks `retained_user_message_tokens`
   (an *output* retention knob), and `bound_compacted_history_to_context` bounds
   only the **result** of summarization. Nothing bounded the **fork itself**.
3. `/compact` is normally run when the context is already near the model window,
   and model-switch compaction targets a *different* (often smaller) window.
   The summary request therefore exceeded the summarizer's window, the provider
   rejected it, and `.context("Failed to generate compaction summary")?` aborted
   the whole command.

### Secondary defect: the error message was lossy

The slash command printed anyhow's plain `Display`, which renders only the
**outermost** context. The outermost context is literally
`"Failed to generate compaction summary"`, so the real provider error (the
actual cause) was discarded — which is why the reported message was
undebuggable.

## Evidence

- Log: `~/.vtcode/sessions/debug-session-vtcode-20260816t130623z_724352-43328.log`
  ```
  ERROR vtcode_transcript: Model switched, but context compaction failed:
    Failed to generate compaction summary. Continuing with full history.
  ```
  Fired immediately after a switch from `deepseek/deepseek-v4-flash-0731` to
  `gemini-3.7-flash` (`compaction_mode=local`).
- Same log, minutes earlier: the **same** provider/model compacted fine with a
  tiny history (`turn=2`, `history_snapshot_bytes=2404`, `tool_count=0`).
  Large-history / small-window only → consistent with an input-size overflow,
  and inconsistent with a schema or `tool_choice` defect.
- Call sites of the wrapper context: `compaction/mod.rs` (legacy
  `compact_history_with_budget` and the manual `summarize_locally` path).

## Changes applied

| File | Change |
| --- | --- |
| `crates/codegen/vtcode-core/src/compaction/mod.rs` | New `bound_history_for_summarization(history, instructions, budget)`: keeps the newest **complete protocol groups** that fit the budget and drops the oldest; reuses `complete_protocol_group_prefix` so an unanswered trailing tool call is excluded rather than shipped as an invalid protocol suffix; degrades to `bounded_protocol_group` previews when no group fits (e.g. one oversized tool result). |
| `crates/codegen/vtcode-core/src/compaction/mod.rs` | `summarize_locally` (the `/compact` + model-switch path) now summarizes `summary_source`, the bounded fork, instead of raw `history`. |
| `crates/codegen/vtcode-core/src/compaction/mod.rs` | The legacy `compact_history_with_budget` path is bounded the same way. |
| `crates/codegen/vtcode-core/src/compaction/mod.rs` | `mod summarization_fork_bounds_tests` — 4 unit tests covering: fits-verbatim, unknown-budget-verbatim, over-budget trimming, no-group-fits fallback. |
| `src/agent/runloop/unified/turn/session/slash_commands/compact.rs` | `format!("... {err}")` → `format!("... {err:#}")` so the full anyhow context chain (the real provider error) is surfaced. |

## Remaining work

1. **Clear `clippy::needless_range_loop`** (the current `check-dev.sh` failure).
   In `bound_history_for_summarization`, replace
   ```rust
   for position in (0..group_starts.len()).rev() {
       let start = group_starts[position];
   ```
   with an iterator form such as
   ```rust
   for (position, start) in group_starts.iter().enumerate().rev() {
       let start = *start;
   ```

2. **Fix the trimming test's budget.** `trims_oldest_groups_when_over_budget_and_keeps_the_newest_turn`
   currently hardcodes `let budget = 3_000;` and panics on
   `assert!(bounded.len() < history.len())`. The hardcoded budget is too
   generous: `Message::estimate_tokens` returns far fewer tokens per character
   than assumed (≈6 chars/token, not ≈4). Derive the budget from the measured
   estimate instead so the test does not depend on the exact heuristic:
   ```rust
   let budget = total_tokens(&history) / 2;
   ```
   Apply the same treatment to `falls_back_to_protocol_previews_when_no_group_fits`
   (replace `Some(600)` with a derived value, and assert
   `total_tokens(&bounded) <= budget` rather than comparing against the
   unbounded history).
   Current state: `3 passed; 1 failed`.

3. **Re-run the gate:**
   ```bash
   cargo test -p vtcode-core --lib summarization_fork_bounds && ./scripts/check-dev.sh
   ```
   Both must be green.

4. **Bound the hierarchical path.** `summarize_locally_hierarchical` still sends
   un-bounded `abstract_band` / `detail_band`. Each band is at most a third of
   the history so an overflow is much less likely, but it is the same class of
   bug. Route both bands through `bound_history_for_summarization`.

5. **Fix the remaining lossy error logs.** `crates/codegen/vtcode-core/src/compaction/auto.rs`
   and `crates/codegen/vtcode-core/src/core/agent/runner/summarize.rs` still log
   with `%error`, which also prints only the outermost context. Prefer the full
   chain (e.g. `?error`, or `error = %error` on the root cause).

6. **End-to-end check.** Run `/compact` on a long session and confirm the
   reported failure is gone; if it still fails, the now-visible provider error
   identifies the next cause.

## Gotchas discovered

- `./scripts/check-dev.sh` **auto-runs `cargo fmt --all`**, rewriting files in
  place. A patch prepared against pre-format text can go stale mid-flight.
- `anyhow::Error`'s plain `{err}` prints only the outermost context. Use
  `{err:#}` (or the root cause) anywhere compaction errors reach the user or
  logs, or the actionable cause is silently dropped.

## Follow-ups

- Consider bounding the fork *before* the cache-safe prefix is built for the
  provider-native paths too, so native and local strategies share one input
  ceiling (mirrors the existing `effective_context_budget` intersection).
