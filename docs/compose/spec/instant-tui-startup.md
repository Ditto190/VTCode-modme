---
feature: instant-tui-startup
status: delivered
updated: 2026-09-18
branch: feat/instant-tui-startup
commits: a566c7169..working-tree
---

# Instant TUI Startup

## Report

**What was built** — Interactive VT Code TUI launch is split so first paint no longer waits on the full agent runtime. `initialize_session_critical` builds the provider client, one plugin-aware primary-agent discovery pass (`discover_controller_subagents`), a lightweight `ToolRegistry`, resume history, cheap bootstrap metadata, and a seed system prompt. `initialize_session_ui` then spawns the TUI. `hydrate_session_runtime` finishes tool-registry async init/policy/CGP/model-tool projection, system-prompt composition into `ContextManager`, subagent controller, trajectory, dynamic context, and MCP reconfigure. `apply_post_hydration_ui` re-drives agent palette, background refresh, primary-agent header, full-auto banner, and system-prompt budget warning after hydration. The interaction loop only dispatches a model turn after hydration succeeds. Standalone CLI cases do not enter this split.

**Verification**
- `RUSTFLAGS="-D warnings" cargo clippy -p vtcode --bin vtcode -- -D warnings` — PASS
- `cargo nextest run -p vtcode -E "test(prepare_session_bootstrap) or test(initialize_session) or test(hydrate) or test(welcome)"` — PASS (7 tests, including `hydrate_replaces_critical_seed_prompt_and_fills_tools`)
- `./scripts/check-dev.sh` — PASS
- Release PTY first-frame (isolated HOME, `--provider ollama --model llama3`): baseline warm ~930–1046 ms; post-change warm samples 462–473 ms, median **467.8 ms**
- Release standalone warm means: `--version` 9.80→4.90 ms; `--help` 9.90→5.12 ms; `schema tools…` 63.61→34.51 ms (no regression)
- Independent review (general-1) found post-hydrate UI parity gaps; re-review (general-2) confirmed all HIGH/MEDIUM findings FIXED and AC4 MET. Residual LOW: UI re-drive path lacks its own unit test.
- Artifacts: `.vtcode/perf/baseline-pre-change-*.json`, `.vtcode/perf/post-change-final-*.json`

**Journey log**
- Skill discovery was already deferred to `/skills`; the critical-path cost was system-prompt composition, tool-catalog projection, CGP, subagent discovery/controller, and workspace bootstrap scans.
- Default binary feature-gating (eval/acp/mcp/indexer) remains out of scope per product policy in `performance.md`.
- First review taught that UI surfaces built before hydrate must be re-driven after: palette, full-auto banner, budget warning, primary-agent header.
- Critical-path primary-agent selection must use the same plugin-aware discovery as the controller (`discover_controller_subagents`); a plugin-blind pass made hooks/header wrong.
- `ToolRegistry::new` still runs on the critical path; further registry-light split and `apply_post_hydration_ui` unit tests are follow-ups.

## [S1] Problem

Interactive VT Code TUI launch waited on a long serial session-setup path before the first usable frame. `initialize_session` completed tool-registry hydration, system-prompt composition, subagent discovery/controller, CGP wiring, trajectory/dynamic-context setup, and MCP reconfiguration before `initialize_session_ui` spawned the TUI.

## [S2] Design

User-approved goals:

1. **Acceptance metric** — interactive first-frame latency.
2. **Scope** — runtime path only; no cargo feature-gating of heavy crates.
3. **Readiness model** — minimal UI first, then ready; first model turn waits for hydration.

### Critical vs deferred split

1. **Critical path (before first frame)**
 - Provider client, plugin-aware primary-agent discovery, lightweight `ToolRegistry::new`, resume history, cheap `SessionBootstrap` (no workspace language/guideline scans), seed system prompt, execution-context shells.
2. **Deferred hydration (after first frame, before first model turn)**
 - `ToolRegistry::initialize_async`, runtime config, trust policy, CGP, `model_tools`, skill tools, snapshot refresh.
 - Full system-prompt composition + `ContextManager::set_base_system_prompt`.
 - Subagent controller / background restore when enabled.
 - Trajectory logger, dynamic-context init, MCP reconfigure + restart.
 - Bootstrap enrichment (prompt addendum from full workspace scans).
3. **Post-hydration UI re-drive** (`apply_post_hydration_ui`)
 - Agent palette + background refresh when a controller exists.
 - Primary-agent header, full-auto banner, system-prompt budget warning.

### Contracts

- Hydration failures abort the session before a model turn runs.
- After hydration, tool surface/skills/system prompt/primary agent/MCP/permissions match prior post-setup behavior.
- No ThreadEvent, safety, or product-surface changes.
- Standalone `--version`/`--help`/`schema tools` must not regress.
- Opt-in trace phases: `session_setup_critical`, `session_setup_ui`, `session_setup_hydrate`, `session_setup`, `first_ui_render`.

## [S3] Out of Scope

- Feature-gating heavy crates out of the default binary.
- Release profile / LTO / allocator changes.
- Provider network latency and first-run onboarding UX redesign.
- Changing `ThreadEvent` or tool safety policy semantics.
- Unit tests for every `apply_post_hydration_ui` UI branch (LOW follow-up).

## Tasks

- [x] T1: Capture pre-change baselines — acceptance: interactive PTY + standalone warm samples recorded (covers: S2)
- [x] T2: Split interactive session setup into critical path + deferred hydration — acceptance: first UI spawn no longer waits on full system-prompt/tool-catalog/CGP/subagent-controller/MCP rehydrate; first model turn waits for hydration (covers: S2)
- [x] T3: Plugin-aware primary-agent discovery on the interactive critical path — acceptance: critical path uses `discover_controller_subagents`; hydrated runtime still resolves the active primary agent (covers: S2)
- [x] T4: Keep standalone startup cases non-regressed and preserve opt-in startup trace phases — acceptance: standalone medians not worse; trace still emits first-frame milestones (covers: S2)
- [x] T5: Update startup performance docs — acceptance: `docs/development/performance.md` and binary gotchas describe critical vs deferred session startup (covers: S2)
- [x] T6: Verify build/tests and re-measure interactive first-frame impact — acceptance: check-dev + targeted nextest PASS; before/after recorded in Report (covers: S2; depends: T2, T3, T4)
