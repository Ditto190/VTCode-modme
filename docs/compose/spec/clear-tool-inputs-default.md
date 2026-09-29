---
feature: clear-tool-inputs-default
status: in-progress
updated: 2026-09-29
branch: feat/clear-tool-inputs-default
commits: e872b4c5d..e872b4c5d # leave empty while in progress; fill at delivery
---

# Clear Tool Inputs by Default

## Report

## [S1] Problem

`agent.harness.tool_result_clearing` stubs old tool *results* past
`trigger_tokens` (default 40k / keep 2), but `clear_tool_inputs` defaults to
`false`. Assistant `tool_calls[].function.arguments` therefore keeps every
historical `apply_patch` patch body and `write_file` file body on every request
after the paired result was already stubbed.

Edit-heavy coding sessions pay this cost twice: the result is reclaimed, but
the model's own pasted patch/file content — often the largest string in the
transcript — rides every subsequent request until late compaction. This is the
largest remaining first-class token leak in the local-clearing path.

## [S2] Design

Settled: flip the default so input clearing follows result clearing. Keep the
opt-out. Do not change the clearing contract.

### S2-default: `clear_tool_inputs` defaults to true

- In `vtcode-config`, `ToolResultClearingConfig::default()` sets
  `clear_tool_inputs: true`.
- Missing TOML keys must follow the same default: the serde field uses
  `#[serde(default = "default_clear_tool_inputs")]` returning `true` (bare
  `#[serde(default)]` on a `bool` is `false` and would silently keep the leak).
- Explicit `clear_tool_inputs = false` remains a supported opt-out.
- No change to `clear_old_tool_results` semantics: inputs are replaced only for
  call ids whose paired results were stubbed, with the existing valid-JSON
  placeholder `{"cleared":"tool_input"}`. Tool name stays on
  `call.function.name`. Freeform `text` and `thought_signature` are cleared the
  same way as today.
- Anthropic mutual exclusion unchanged: local clearing (results + inputs) runs
  only when the wire will not carry native `clear_tool_uses`.
- Durable history and `vtcode-exec-events::ThreadEvent` remain untouched
  (request-only shaping).

### S2-docs: state the new default

- `docs/config/CONFIG_FIELD_REFERENCE.md`: default `false` → `true`.
- `docs/development/EXECUTION_POLICY.md` tool-result-clearing bullet: note that
  paired tool *inputs* are cleared by default as well.

## [S3] Out of Scope

- Changing `trigger_tokens` / `keep_tool_uses` / `clear_at_least_tokens`.
- Compaction thresholds or compaction prompts.
- Wire-level Anthropic `clear_tool_uses` behavior.
- Placeholder shape changes or path-preserving input stubs.
- Startup warning text beyond what already covers disabled clearing.

## Tasks

- [ ] T1: default `clear_tool_inputs` to true (struct Default + serde default fn) — acceptance: `ToolResultClearingConfig::default().clear_tool_inputs` is true; a TOML harness config that omits the key also yields true; explicit `false` still parses (covers: S2-default)
- [ ] T2: update pin tests + docs — acceptance: `test_tool_result_clearing_defaults` asserts true; CONFIG_FIELD_REFERENCE and EXECUTION_POLICY show the new default (covers: S2-docs; depends: T1)
- [ ] T3: regression that cleared results clear paired inputs by default — acceptance: `clear_old_tool_results` with default-on config replaces arguments for stubbed call ids and leaves kept calls intact (covers: S2-default; depends: T1)
