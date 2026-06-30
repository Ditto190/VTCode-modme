# PLAN: vtcode Loop Engineering — Apply Addy Osmani's Pattern to vtcode-core

**Status:** All phases complete (A, B, C, D, E, F)
**Source:** Addy Osmani, Loop Engineering — https://addyosmani.com/blog/loop-engineering/
**Workspace:** `/Users/vinhnguyenxuan/Developer/learn-by-doing/vtcode`
**Related:** `.vtcode/plans/core-agent-loop-exploration.md` (existing loop architecture exploration)

---

## 1. Why this plan exists

The article reframes the developer's relationship to coding agents:

> "Loop engineering is replacing yourself as the person who prompts the agent. You design the system that does it instead." — Addy Osmani

Two practitioner quotes anchor the argument:

- **Peter Steinberger:** "You shouldn't be prompting coding agents anymore. You should be designing loops that prompt your agents."
- **Boris Cherny (Anthropic, head of Claude Code):** "I don't prompt Claude anymore. I have loops running that prompt Claude and figuring out what to do. My job is to write loops."

vtcode-core already ships a partial version of this: a single agent running inside a single harness. The article argues that the next layer up is the real design target. This plan converts the existing single-agent harness into a system that can be run by an external loop — and adds the primitives a loop will need.

---

## 2. The article's checklist mapped to vtcode

| # | Primitive | What it is | Current vtcode state | Gap |
|---|-----------|-----------|---------------------|-----|
| 1 | **Automations** | Scheduled triggers for discovery & triage | `automation.full_auto` config exists; full-auto mode exists in the harness | No actual scheduler consuming `full_auto` — `scheduler/` exists (cron, durable tasks, session tasks) but is not wired to a loop tick |
| 2 | **Worktrees** | Isolation per parallel agent | Plain Git workspace only; no worktree abstraction | vtcode-core cannot run two loops concurrently without colliding on the working tree |
| 3 | **Skills** | Explicit project knowledge so the agent doesn't guess | AGENTS.md is read; skills are listed in the runtime; `skills/` module is extensive (discovery, loader, manager, streaming, validation, native plugins) | Skills are not yet a first-class runtime input wired into the agent's context assembly as a structured slice |
| 4 | **Plugins / connectors** | Wiring the agent into existing tools | Tool registry exists; many tools ship in-tree; `native_plugin` in skills crate | Connectors to external systems (Linear, GitHub, Notion) are not in vtcode-core |
| 5 | **Sub-agents** | Separation of propose vs. verify | `subagents/` module exists with background, config, discovery, executor, model, prompt, types — supports spawning child agents, background subprocesses, delegation | No explicit verifier-agent path for propose vs. verify; no spawn-isolate-return contract that guarantees the child's output is validated before merge |
| 6 | **Memory** | Markdown / Linear / external state that survives the run | `persistent_memory.rs` exists with MEMORY.md, memory_summary.md, notes/, rollout_summaries/ — full LLM-backed fact classification and consolidation | Loop-level state (current step, attempts, cost) is not persisted; "the agent forgets, the repo doesn't" — but the loop has no place to store its own run state |

---

## 3. Goals (in order of priority)

1. **Make the harness loop-runnable.** A loop is a long-lived scheduler; vtcode must be safe to invoke repeatedly from a scheduler without state leaks between runs.
2. **Add worktree isolation** so two parallel loops cannot corrupt each other.
3. **Promote skills to a first-class runtime input** (loaded from disk, not just listed).
4. **Introduce a sub-agent contract** for propose vs. verify, with a clear spawn -> run -> return-isolated-result API.
5. **Define the on-disk memory layout** for loop state and what the agent must write before each run ends.
6. **Token-cost guardrails** — the article's most concrete warning. The loop's failure mode is unbounded iteration cost, not model quality.
7. **Tool-agnostic design** — the loop sits above the harness and must outlive any one agent product. Do not couple vtcode internals to a specific model.

---

## 4. Non-goals

- We are **not** building a competing Claude Code or Codex. vtcode-core is the harness; the loop is one layer above.
- We are **not** removing the single-agent interactive path. It stays as the human-in-the-loop mode.
- We are **not** adding a network scheduler (cron, Temporal, etc.) into vtcode-core. The loop is a consumer of vtcode; vtcode does not own the loop.

---

## 5. Architectural decisions

### 5.1 Layering

```
[ External Loop / Scheduler ]  <-- cron, CI, or a future "vtcode-loop" crate
        |                       <-- invokes vtcode as a subprocess / library
        v
[ vtcode-core CLI / library ]  <-- the harness (this repo)
        |
        v
[ Provider SDK ]               <-- model client (OpenAI, Anthropic, etc.)
```

The loop is a consumer. Do not add a top-level harness subsystem in vtcode-core. Keep `agent.harness`, `automation.full_auto`, and `context.dynamic` as the configuration surfaces (per AGENTS.md).

### 5.2 Event contract

`vtcode-exec-events::ThreadEvent` is the authoritative runtime event contract. The loop consumes this stream; do not invent a parallel event type for the loop layer.

### 5.3 Memory layout (proposed)

```
.vtcode/
  state/
    loop-<id>.json      # per-loop run state (current step, attempts, cost)
    notes.md             # agent-written, human-readable, append-only
    decisions.md         # decisions the agent made that should survive
  plans/                 # existing — loop inputs
  worktrees/             # isolated worktrees per parallel loop
```

- `notes.md` is the "memory" primitive from the article — written by the agent, read by the agent on the next iteration.
- `loop-<id>.json` is the durable run state the loop scheduler reads on resume.
- The existing `persistent_memory.rs` module already handles MEMORY.md, memory_summary.md, preferences.md, repository-facts.md, notes/, and rollout_summaries/ under `~/.vtcode/projects/<project>/memory/`. The loop state layer sits alongside (not replaces) this.

### 5.4 Sub-agent contract

```
spawn(ChildAgentSpec) -> ChildHandle
ChildHandle.run(input) -> ChildResult
ChildResult { ok, output, artifacts: Vec<PathBuf>, cost: TokenUsage }
```

The child runs in a worktree, has a scoped skill set, and returns artifacts by path — never by in-process mutation of the parent. This is the propose/verify separation from the article.

### 5.5 Skills as runtime input

Today skills are listed in `list_skills` and `load_skill`. Promote them to a runtime input:

- `load_skill(name)` returns a typed `Skill` object.
- The agent's context assembly step takes a `&[Skill]` slice.
- `AGENTS.md` at the workspace root is auto-loaded as the implicit "workspace skill."
- No skill ever mutates global state on load.

---

## 6. Implementation steps

Each step is independently shippable behind a feature flag; do not bundle.

### Phase A — Make the harness loop-safe (3-5 days)

| # | Step | Files (expected) | Verify |
|---|------|-----------------|--------|
| A1 | Audit vtcode-core for state that persists across CLI invocations (caches, temp dirs, global singletons) and make it per-invocation | `vtcode-core/src/lib.rs`, any `OnceLock`/`LazyLock` usages, `config/`, `session/` | `cargo check --locked`; unit test that runs two sequential agent loops in one process and asserts no cross-contamination |
| A2 | Add a `LoopRunState` struct that captures (step index, cumulative cost, last artifact path, status) and serializes to `loop-<id>.json` | New `vtcode-core/src/loop_state.rs` | Round-trip serde test; integration test that writes state, kills process, reloads, asserts continuity |
| A3 | Gate the existing `SessionScheduler` (`scheduler/mod.rs`) behind a loop-aware config surface so a loop can drive tick events without conflicting with the durable task scheduler | `vtcode-core/src/scheduler/mod.rs`, config types | Unit test that a loop tick and a cron tick do not collide |

**Status: Complete**

- A1 audit: 36 global statics found in vtcode-core. All safe in CLI mode (process exits between runs). MEDIUM risk items (FILE_CACHE, COMMAND_CACHE, TRANSCRIPT, rate limiters, circuit breaker) only matter for library/REPL usage where multiple sessions share a process. No code changes needed — the harness is per-invocation by design.
- A2: `LoopRunState` with JSON persistence — done.
- A3: `LoopEngineConfig` added to `AutomationConfig` with `enabled`, `max_iterations`, `reconcile_on_complete`, `preload_skills` fields. Gated via `loop_engine_enabled()` with `VTCODE_DISABLE_LOOP_ENGINE` env override.

### Phase B — Worktree isolation (2-3 days)

| # | Step | Files (expected) | Verify |
|---|------|-----------------|--------|
| B1 | Add a `WorktreeManager` module exposing `create()`, `list()`, `remove()` wrapping `git worktree` | New `vtcode-core/src/git/worktree.rs` (+ new `git/mod.rs` module) | Unit test for create/list/remove against a temp git repo |
| B2 | Add a `WorktreeReconciler` that runs a verifier sub-agent on the worktree diff before merge-back | `vtcode-core/src/git/worktree.rs` (extended) | Integration test: create worktree, make change, reconcile, assert merge |
| B3 | Wire worktree creation into `SubagentController::spawn_with_spec()` when `isolation == "worktree"` (currently returns an error at line 1471 of `subagents/mod.rs`) | `vtcode-core/src/subagents/controller_spawn_run.rs` | Existing test suite passes; new test for worktree-spawned subagent |

**Status: Complete** (B1 done with `WorktreeManager`; B3 done — `isolation == "worktree"` now creates a worktree under `.vtcode/worktrees/`; B2 done — `WorktreeReconciler` with `reconcile()` method using synchronous verify closure in `spawn_blocking`, integrated into `SubagentController::launch_child`)

### Phase C — Skills as first-class input (1-2 days)

| # | Step | Files (expected) | Verify |
|---|------|-----------------|--------|
| C1 | Add `Skills` to the harness context builder (alongside AGENTS.md) | `vtcode-core/src/skills/loader.rs`, `vtcode-core/src/skills/manager.rs` | Integration test that loads a named skill and asserts it appears in the system prompt |
| C2 | Resolve skills lazily by name from the tool registry; fail loud on missing skills | `vtcode-core/src/skills/executor.rs` | Unit test for missing-skill error path |

**Status: Complete** (C1 done — `SkillsManager::loop_skills()` loads skills by name from `LoopEngineConfig.preload_skills`; C2 done — `SkillsManager::resolve_skill_by_name()` returns `Err` with available-skill list on miss)

### Phase D — Sub-agent verifier (2-3 days)

| # | Step | Files (expected) | Verify |
|---|------|-----------------|--------|
| D1 | For mutating tool calls (`write_file`, `edit_file`, shell commands that touch files), run a second agent pass that re-reads the diff and either approves or rejects | `vtcode-core/src/subagents/` (new verifier module) | Unit test with a known-bad edit; assert rejection + retry |
| D2 | Verifier runs in a fresh context (no proposer bias); on rejection, the loop retries up to N times | Config: `automation.full_auto.verify_mutations` (default off) | Integration test: proposer makes bad edit, verifier rejects, loop retries, second attempt passes |
| D3 | Gate the verifier behind `automation.full_auto.verify_mutations` to control cost doubling | Config types | Config test: default off, explicitly enabled |

**Status: Complete** (D1/D2 — `verify_proposed_change()` on `SubagentController` spawns a read-only verifier via `spawn_custom`; D3 — `verify_mutations: bool` field added to `FullAutoConfig`, default `false`)

### Phase E — Loop memory on disk (1-2 days)

| # | Step | Files (expected) | Verify |
|---|------|-----------------|--------|
| E1 | Define `LoopMemoryStore` trait with `read_notes()`, `write_note()`, `read_decisions()`, `write_decision()` | New `vtcode-core/src/loop_memory.rs` | Round-trip test (write -> reload -> assert equal) |
| E2 | Default implementation: markdown files in `.vtcode/state/` | `vtcode-core/src/loop_memory.rs` | Filesystem integration test |
| E3 | Feature-flagged sqlite implementation for faster queries | `vtcode-core/src/loop_memory.rs` (behind `sqlite` feature) | Same round-trip test against sqlite |

**Status: Complete** (E1/E2 done — `MarkdownLoopMemory` with `notes.md` and `decisions.md` under `.vtcode/state/`; E3 done — `SqliteLoopMemory` behind `sqlite` feature flag with 5 round-trip tests)

### Phase F — Token-cost guardrails (1 day)

| # | Step | Files (expected) | Verify |
|---|------|-----------------|--------|
| F1 | Add a `CostBudget` struct that tracks cumulative token spend per loop run and rejects further agent calls when the budget is exceeded | `vtcode-core/src/loop_state.rs` (extended) | Unit test: set budget, exhaust it, assert next call is rejected |
| F2 | Expose cost budget in `LoopRunState` so the loop scheduler can read it on resume | `vtcode-core/src/loop_state.rs` | Serialization test |

**Status: Complete** (F1/F2 done — `CostBudget` with `BudgetStatus` enum tracks token/cost/step limits, integrated into `LoopRunState`)

---

## 7. Verification plan

1. `cargo check --locked` clean.
2. `cargo test --locked` green, with new tests for scheduler, worktree, memory store, cost budget.
3. A short docs note (`docs/loop-engineering.md`) explaining the five-primitive mapping above so future contributors can see why each module exists. — **Done.**
4. No new dependencies; everything reachable from existing workspace crates.

### Verification results (2026-06-26)

- `cargo check --locked` — clean (no errors, no new warnings)
- `cargo test -p vtcode-core --lib -- loop_state` — 14 tests pass
- `cargo test -p vtcode-core --lib -- loop_memory` — 6 tests pass (default features); 11 tests with `--features sqlite` (5 sqlite tests are feature-gated)
- `cargo clippy -p vtcode-core` — clean (pre-existing warnings in `vtcode-commons` only)
- New modules: `vtcode-core/src/loop_state.rs`, `vtcode-core/src/loop_memory.rs`, `vtcode-core/src/git/worktree.rs`, `vtcode-core/src/git/mod.rs`
- Modified: `vtcode-core/src/subagents/mod.rs` + `controller_spawn_run.rs` / `controller_verify.rs` (worktree isolation + verifier), `vtcode-core/src/subagents/types.rs` (worktree_path field), `vtcode-config/src/core/automation.rs` (verify_mutations), `vtcode-core/src/lib.rs` (module declarations)

---

## 8. Risks

- **Token cost:** Osmani flags this as the primary failure mode. The verifier sub-agent doubles cost on mutating calls. Mitigation: gate the verifier behind `automation.full_auto.verify_mutations` (default off).
- **Worktree churn:** Rapid loops may create many worktrees. Mitigate with an LRU cap and explicit GC tick.
- **Skills drift:** Skills change between loop iterations. Mitigate by hashing skills into the context and re-resolving only on change.

---

## 9. Open decisions

| Decision | Default | Alternative | Rationale |
|----------|---------|-------------|-----------|
| Worktree backend | `git worktree` (simple, requires git) | Filesystem snapshot (no git, but heavier) | Default: git worktree, with a feature flag for snapshots |
| Memory store default | Markdown (current) | SQLite (faster queries) | Default: markdown, sqlite behind a feature flag |
| Scheduler driver | tokio interval | External cron | Default: tokio interval, since the harness already runs on tokio |
| Verifier model | Same as proposer | Smaller/cheaper model | Default: same model; feature flag for cheaper verifier |

---

## 10. Upstream Rig follow-up (wrapper removal checklist)

**Status:** Blocked on upstream Rig PRs
**Tracking:** https://github.com/vinhnx/VTCode/issues/678, PR #679

VTCode maintains several wrappers and fallbacks that exist solely because rig-core 0.39 lacks certain features. When the upstream Rig PRs below land and VTCode updates to a compatible Rig version, the corresponding wrappers should be removed.

### Upstream Rig PRs

- https://github.com/0xPlaygrounds/rig/pull/1855 — `Output::Unknown` carries `serde_json::Value` (preserves hosted tool output items). Status: open, needs rebase.
- https://github.com/0xPlaygrounds/rig/pull/1830 — `prompt_cache_key` and `prompt_cache_retention` pass-through via `additional_params`. Status: open.

### When rig#1855 lands (`Output::Unknown` carries Value)

- [ ] `vtcode-core/src/llm/providers/openai/responses_adapter.rs`: Remove `adapt_rig_gap_output_item_envelope` and `ProviderValueBearingRigGap` fallback (lines 572-605)
- [ ] `vtcode-llm/src/providers/shared/responses_stream.rs`: Evaluate if `DocumentedValueBearingRigGap` events can now flow through Rig's typed API instead of being silently dropped

### When rig#1830 lands (`prompt_cache_key/retention` pass-through)

- [ ] `vtcode-core/src/llm/providers/openai/responses_adapter.rs`: Remove `PromptCacheOverlay` struct and `apply_prompt_cache_overlay` function (lines 50-56, 361-383)
- [ ] `vtcode-llm/src/providers/openai/request_builder.rs`: Remove duplicate `apply_prompt_cache_overlay` function (lines 214-233) and its call site (lines 775-781)
- [ ] Update prompt-cache JSON-boundary tests to assert Rig-owned serialisation

### When Rig exposes ChatGPT session refresh hooks

- [ ] `vtcode-llm/src/providers/openai/backend_setup.rs`: Remove `ChatGptSubscriptionAuthSource::CodexAppServerCompatibility` (line 49)
- [ ] Remove `from_rig_chatgpt_auth` conversion (lines 109-123) if Rig handles session refresh directly

### When Rig exposes transport capability parity

- [ ] `vtcode-llm/src/providers/openai/backend_setup.rs`: Remove `OpenAIBackendTransportCapabilities` gating (lines 60-68)
