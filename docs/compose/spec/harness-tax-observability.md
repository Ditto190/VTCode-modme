---
feature: harness-tax-observability
status: in-progress
updated: 2026-09-27
branch: feat/harness-tax-apply
commits: # leave empty while in progress; fill at delivery
---

# Harness Tax Observability

## Report

## [S1] Problem

HarnessTax (Pan et al., https://harnesstax.github.io/) shows the same model can cost up to 5x more under a different coding-agent harness at essentially the same success rate. Most of that gap is **first-call fixed overhead** (instructions + tool schemas), not more turns. VT Code already budgets this (Progressive schemas ≤ ~2.2k tokens, first request ≤ 12k) and logs `token_budget_breakdown` per turn, but:

1. Users cannot see the first-call fixed overhead ("harness tax") without reading trajectory logs. The exit summary reports session totals only.
2. Eval reports expose `cost_usd` totals and pass@k, but not **cost per solve**, **tokens per attempt**, or **turns per attempt** — the figures the paper uses to compare harnesses. VT Code therefore cannot be placed on a cost-success frontier the way the study places Pi / Codex / Claude Code.

## [S2] Design

Settled scope (grill 2026-09-27): observability + eval metrics; means and cost_per_solve only (no bootstrap CIs).

### S2-first-call: first-call harness-tax composition

- Reuse the existing per-turn `token_budget_breakdown` measurement in
  `src/agent/runloop/unified/turn/turn_processing/llm_request/` (system prompt tokens, tool-schema tokens, message-history tokens).
- Add two derived fields to the breakdown record and log line:
  - `first_call: bool` — true when this is the session's first assembled LLM request (`step_count == 1` and no prior successful request build this session).
  - `fixed_overhead_tokens: usize` — `system_prompt_tokens + tool_schema_tokens`. This is the harness tax per call; on the first call it is the paper's initial harness context (excluding the task prompt).
- Capture the first-call composition once into the interactive `SessionStats` (`src/agent/runloop/unified/state.rs`) as `first_call_composition: Option<FirstCallComposition>` with `system_prompt_tokens`, `tool_schema_tokens`, `message_history_tokens`, `on_wire_tools`, `fixed_overhead_tokens`.
- Surface it once on session exit in `postamble::build_stats_line` / `ExitData` as a stats fragment when present, e.g. `First-call overhead 3.2k (system 1.1k + tools 2.1k)`. Zero-valued totals omit the fragment. No new slash command.
- Do not change request payload shape, tool schemas, or prompt text. This is measurement + display only.

### S2-eval: cost-efficiency metrics in eval reports

In `crates/codegen/vtcode-eval`:

- Aggregate per suite while already summing `cost_usd` and merging `trace_summary`:
  - `mean_cost_per_attempt: Option<f64>` — `all_cost_usd / known_cost_runs` when `known_cost_runs > 0`.
  - `cost_per_solve: Option<f64>` — `all_cost_usd / passed_runs` when `passed_runs > 0` **and** every run that contributed cost is accounted for (`unpriced_runs == 0`). Unpriced runs keep the metric `None` rather than treating them as free (matches the existing "unknown cost is not zero" rule).
  - `mean_tokens_per_attempt: Option<f64>` — mean of `(input_tokens + output_tokens)` over attempts that carry a `trace_summary`; `None` when no trace summaries exist.
  - `mean_turns_per_attempt: Option<f64>` — mean of `trace_summary.turns` over attempts that carry one.
- `passed_runs` is attempt-level (count of `EvalRunResult` with `RunOutcome::Pass`), matching the paper's resolve-rate denominator (`total cost / successful attempts`).
- Markdown report line under the aggregate: `Cost/solve $X.XXXX · $Y per attempt · Z tokens · W turns` when the values exist.
- Serialize the new fields on `SuiteReport`. Do not change `EvalMetric` / pass@k / pass^k shapes.

### S2-docs: research mapping

- Record the HarnessTax claims and the VT Code counterpart in `.vtcode/memory/library.md` (repo-local).
- Add a short "Harness tax" subsection to `docs/development/EXECUTION_POLICY.md` (Auditing token cost area) naming the first-call breakdown fields and the eval metrics, so the feature stays discoverable.

## [S3] Out of Scope

- Bootstrap confidence intervals on cost/success (deferred).
- A SWE-bench / Terminal-Bench runner or cross-harness comparison harness.
- Changing the default tool surface, Progressive schema budgets, or SystemPromptMode defaults.
- New config profiles or `/harness-tax` slash command.
- Provider cache-affinity work (already delivered in `harness-stability-cost-p1` / `provider-cache-affinity-p2`).

## Tasks

- [ ] T1: first-call composition fields on token_budget_breakdown + SessionStats capture — acceptance: unit/serial tests show `first_call` true only on the session's first request build, `fixed_overhead_tokens == system + tools`, and the captured `FirstCallComposition` is recorded once (covers: S2-first-call)
- [ ] T2: exit-postamble first-call overhead fragment — acceptance: `build_stats_line` includes `First-call overhead …` when composition is present and omits it when absent/zero; existing stats-line tests still pass (covers: S2-first-call; depends: T1)
- [ ] T3: eval suite cost-efficiency metrics — acceptance: tests cover cost_per_solve (all priced + at least one pass), None when unpriced runs exist, mean tokens/turns from trace summaries, None when no traces; markdown report renders the new line (covers: S2-eval)
- [ ] T4: docs + library entry — acceptance: EXECUTION_POLICY harness-tax subsection and library.md entry cite HarnessTax and name the new fields (covers: S2-docs; depends: T1, T3)
