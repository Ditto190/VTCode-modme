Areas that are complex
The most intricate parts are likely:

     1. core_tui/session.rs
        Main state transitions, layout, rendering, and interaction coordination.
     2. Transcript rendering and caching
        Reflow, scroll behavior, tool blocks, PTY output, overlays, and cache
        invalidation interact heavily.
     3. Input ownership
        Normal input, popups, approval prompts, search, and fullscreen review each have
        different routing rules.
     4. Async integration
        Terminal events, agent events, PTY events, and redraw requests must be
        coordinated without blocking the runtime.
     5. Theme and contrast behavior
        Theme changes affect normal text, accents, syntax highlighting, status colors,
        overlays, and accessibility requirements.

==> improve

=====

===

1. tui/core*tui/session.rs:1-407 — state transitions / layout
   Split across session/state.rs:1-958, impl_init.rs, impl_layout.rs, impl_render.rs, driver.rs, action.rs, config.rs. Gotcha: transcript_area is source of truth for scroll/hit-test, ActivityState is global busy/idle, bottom-half rect shared by overlays + clipping.
   reasoning.effort=xhigh. Map Session lifecycle in tui/core_tui/session.rs + session/state.rs + session/driver.rs + session/action.rs.
   Output: state table (field, mutated where, dirty/redraw path, modal/timeline interaction) + top 5 transition bugs (e.g. mark_dirty missed, overlay vs transcript_area clipping, ActivityState drift). Keep transcript_area as scroll source, no explicit rect to apply_view_rows. Patch surgically, verify with cargo nextest run -p vtcode-ui.
   Decompose Session god-object: propose extract of init/layout/render/scroll/style from impl*\*.rs into small interfaces without breaking task_panel.rs helpers or SessionWidget in tui/ui/tui/widgets/. Keep 4-space, anyhow::Result+context. Show before/after file:line.
2. Transcript rendering + cache
   session/transcript.rs:1-585 (TranscriptReflowCache, revision, invalidate_message), reflow/blocks.rs|formatting.rs|helpers.rs, wrapping.rs, tool_renderer.rs, message_renderer.rs, widgets/transcript.rs. Gotchas: info/warning/error blocks invalidate from first line, each Info summary line is boundary, Alt+T must invalidate caches, PTY • prefix keeps explicit status color, blank line above/below tool blocks, hit regions rebuilt after reflow.
   reasoning.effort=max. Audit TranscriptReflowCache: set_width/invalidate_content/needs_reflow/update_message in session/transcript.rs + reflow/ + wrapping.rs + text_utils.rs hanging-prefix.
   Find: stale revision on grouped reflow, width-change over-invalidation, scroll-anchor loss, PTY live vs complete capture leak, compact review hint hit-region drift. Fix with bounded cache + targeted invalidate_message, add test in session/tests/transcript_rendering.rs + diff_overlay.rs. Measure large-transcript reflow time before/after.
   Repro PTY/scroll bugs end-to-end: compact vs expanded PTY, Ctrl+T Transcript Review (rich/raw via r), drag_autoscroll, overlay_list scroll-through. Use computer-use to drive terminal resize + wheel outside modal_list_area (must pass to transcript). Output failing case as nextest + bounded live lines, complete captures behind viewer.
3. Input ownership
   session/input_manager.rs:1-1234 (TextArea wrapper, max_histories 50), input.rs, impl_input.rs, textarea_bridge.rs, modal/state.rs|render.rs|layout.rs, reverse_search.rs, queue.rs. Gotchas: bridge prompts = deferred-event queue prompt-only, transient overlays own input, slash parsing terminal-only, toggle_tool_display_mode Alt+T before legacy text-edit, Ctrl+C copy-swallow, fullscreen Ctrl+T dual meaning.
   Build input-routing matrix for: normal, popup/modal, approval, reverse-search, fullscreen review, queue_inputs. Trace in input_manager.rs + impl_input.rs + modal/ + reverse_search.rs + queue.rs. Find focus leaks, key swallowing (Alt+T, Ctrl+C/T), mouse ownership outside modal_list_area. Unify to explicit owner enum + guard, add tests in tests/vim.rs,input_navigation.rs,queue_inputs.rs,overlay_list.rs.
4. Async integration
   runner/events.rs:1-278 (TerminalEvent::Tick/Crossterm, EventChannels pause/resume, last_input_elapsed_ms), runner/drive.rs|surface.rs|terminal_io.rs|signal.rs|terminal_modes.rs, session/events.rs|impl_events.rs, panic_hook/. Gotchas: terminal-op lock for render/finalize, alt-screen teardown clears before leave, panic-hook only after successful mutation, active-PTY counter = global loading observer with footer fallback in state.rs.
   reasoning.effort=xhigh. Audit event fan-in: Tick adaptive rate via last_input_elapsed_ms, agent/PTY/redraw coordination in runner/drive.rs + runner/events.rs + session/impl_events.rs. Find: blocking recv, unbounded mpsc queue (clear_queue drain), missed pause/resume, signal teardown race, redraw starvation. Fix with bounded coalesced diagnostics, non-blocking handoff, terminal-operation lock proof. Verify with harness PTY tests: cargo nextest run -p vtcode-core -E 'binary(/pty_tests/)'.
5. Theme + contrast
   theme/registry.rs:52-923, scheme.rs, runtime.rs, color_math.rs, tests.rs:58-80. Requirement: all built-ins WCAG AA 4.5:1, cargo nextest run -p vtcode-ui -E 'test(theme)'. Catppuccin-latte special-case saturating_sub(32).
   For every theme in all_theme_definitions: validate foreground/primary/secondary/user/response vs background via contrast_ratio in theme/tests.rs. Theme changes must propagate to normal/accent/syntax/status/overlay. Fix latte-style failures by darkening, not lightening bg. Add snapshot test in tui/core_tui/widgets/snapshots/ for tool success/failure/warning • prefix + syntax fallback when shell highlight yields no distinct tokens.
   Run order: 1 -> 2 -> 3 -> 4 -> 5. Delegate 2+5 in parallel subagents — no shared files.

===

Refined plan (locked decisions applied) 0. Goal + guardrails
Engine gpt-6-astra, whole harness model-agnostic. No if model=="..."; only ResolvedModel::context_window()/pricing()/reasoning_supported(), Provider trait, ModelCatalogEntry.

- ThreadEvent 0.14.0 frozen — no new variants; unify via manifest referencing thread.compact_boundary, context.reset, turn.blocked.
- Style: 4-space, anyhow::Result+with_context, CompactString, surgical diffs, no new top-level harness subsystem.
- Verify each PR: ./scripts/check-dev.sh --changed + cargo nextest run (never cargo test) + 3-family log (OpenAI + Claude + Gemini/local).
- Execution: parallel subagents per workstream, one verifier unless failed.
  Locked: 7 findings → 7 small issues; scope = docs + High fixes; P1 160k default; P5 fail-closed + allow_unpriced=false; P8 run_suite stays sequential.

1. File 7 small issues (no code)
   Restore deleted diagnosis as tracked issues: cold cache, preview_budget_exhausted+spool_path:null, completion_state:unknown on exit-0, 19k/44k visible/spooled gap, low_signal_tool_calls:0 on 3×find, elapsed_ms:null, files:[].
2. P3 cache + P1 budget denominator (first, parallelizable)

- core/agent/hash_utils.rs:19-62,229-251: keep PromptCapabilityIdentity; replace raw model string with capability digest (model_id canonical + ResolvedModel::context_window + reasoning_tag + catalog_epoch + parallel/cache/tools bools) via FNV-1a hash_value; strip [Harness Limits]/Environment/ShellProfile from stable prefix or freeze per segment (request_envelope.rs:12-21,45-108, state.rs:206-282, request_builder.rs:230-253).
- Fix prompts/system.rs:576-609 cache_key: DefaultHasher → StableHasher, require explicit epoch, include digest.
- P1: default context.max_context_tokens=160k (align context.rs:196-198 0 with constants/tool_limits + docs agent-loop-contract.md:204); effective_budget=min(provider, session, safety) in memory_envelope.rs:1276-1362, task_setup.rs:131-135, execute.rs:509-557; log denominator/turn; 90% of min() threshold.
- Tests: same-prompt × 3 families → same stable prefix, distinct digest; Usage::cache_hit_rate (commons/llm.rs:72-84) hit on 2nd turn; 32k vs 1M auto-compact (turn/compaction/tests.rs:2078-2145).

3. P2 effort + P5 cost (parallel subagents)

- P2: reuse reasoning_effort.rs:20-99 ReasoningEffortMapper; refactor rig_adapter.rs:25-144 to query provider_trait.rs:24 supports_reasoning_effort + supported_reasoning_efforts:187-195; degrade Max→XHigh→High with turn.blocked diagnostic, honor allow_reasoning_effort_downgrade=false (agent.rs:99-101). Matrix test all levels × OpenAI/Anthropic/Gemini.
- P5: keep usage_cost.rs:55-102 canonical (raw=enforcement, effective=display); execute.rs:789-840: pricing None + max_budget set + !allow_unpriced → TurnBlocked, else explicit warn; replace defaults.rs:7-17, openai/errors.rs:72-78, lightweight_routing.rs:179-201, orchestrator_retry.rs:150-179 hardcodes with catalog-driven (preferred_lightweight_variant, non_reasoning_variant); aggregate cost_usd in eval/task.rs:49, metric.rs, report.rs.

4. P4 guidelines (small)
   prompts/guidelines.rs:22-94: add overload generate_tool_guidelines(level, ResolvedModel); terse Minimal for small/high-cost, Default for large; parallel hint only if supports_parallel_tool_config. Snapshot both. Files: guidelines.rs, system.rs:319-385, harness_limits.rs:14-50.
5. P6 safety gateway (High fixes)
   All at tools/safety_gateway.rs:197-690 + registry/:

- Spool: output_spooler.rs:34-41,191, spool_processing.rs:43-63, file_ops/read.rs:253-256, tool_reads.rs:28-31 substring → canonical containment, no_spool enforced at gateway, readers via SpooledOutputReference only.
- Env: safety/sandboxing/manager.rs:56-62, exec_session.rs:502-506, child_spawn.rs:15-212 denylist → build_sanitized_env allowlist in restrictive.
- Paths: commons/paths.rs:202-287 resolved variant into bash-runner/policy.rs:36-42, skill_policy.rs:257-278, command_validation.rs:1015.
- Shell: bash-runner/executor.rs:164-169, runner.rs:435-437, shell_handler.rs:86-87 join(" ") → argv-only; wire shell_parser.rs:157-264 redirection + sudo/$BIN/curl/python -c into sandbox_runtime.rs:674-843 preflight.
- WebMCP filesystem.rs:550-561,1033-1118 pin system cargo; skills skill_policy.rs:15-63 expand NETWORK_TOOLS; MCP mcp_tool.rs:12-18, mcp/provider.rs:561-575 size cap + namespace + list_changed limit.
- Tests: adversarial nextest + extend pty_tests.rs, pipe_tests.rs:1-276.

6. P7 TUI targeted + P8 eval/memory (last)

- P7: keep facade (session.rs:30-80 + 20 submodules); fix transition table (activity.rs:4-60), reflow invalidation (transcript.rs:28-311, state.rs:139-147,477-512), introduce InputOwner enum (replace input_enabled bools), keep Tick/PTY coalescing (events.rs:12-211, drive.rs:199-512), WCAG 4.5:1 (theme/tests.rs:58-125). Verify nextest -p vtcode-ui -E 'test(theme)' + transcript_rendering + overlay_list.
- P8: true pass@k=1-C(n-c,k)/C(n,k), pass^k=(c/n)^k + attempts>=1 guard in suite.rs; keep executor.rs:33-44 sequential, parallelize at caller with cost/latency join to trace_analyzer/; eviction→summarize hook, BM25 replace substring (query.rs:141-227), LRU invalidate (query.rs:11-14); cross-model regression suite (Astra executes, Claude/Gemini pass).

---

    How to catch up
    Lean into what's differentiated rather than cloning:
    1. Make the eval framework the product. vtcode-eval + the WebMCP eval
       corpus pattern is a real asset. If VT Code can say "here's a
       reproducible benchmark showing my harness gets more tasks done per
       dollar than Claude Code on your repo," that's a wedge the incumbents
       can't easily match — they won't ship their internal evals. Publish
       baseline suites, make vtcode eval a one-command story.
    2. Own "the auditable agent." The security posture + ThreadEvent log +
       vtcode-memory single-source-of-truth could become a story no one else
       tells: every action replayable, every mutation digest-verified, every
       permission decision inspectable. Enterprises care about this more than
       raw model quality.

---

    ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
    Gap: Onboarding polish
    Why it matters: Codex/Claude Code have zero-config first runs.
    Catch-up move: Reduce time-to-first-success: auto-detect providers, ship a guided vtcode init, and make the default config excellent without reading
    docs.
    ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
    Gap: Sub-agent/parallel UX
    Why it matters: Claude Code's Task tool is well-tuned in practice.
    Catch-up move: Your propose/verify + worktree isolation is architecturally stronger — invest in making it visible: better progress surfacing in the TUI,
    and cost/token reporting per sub-agent.
    ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
    Gap: Eval-driven credibility
    Why it matters: Anthropic/OpenAI publish eval numbers; trust follows.
    Catch-up move: Use vtcode-eval publicly: publish benchmark runs per provider/model on your repo. "Verified on these models with these scores" is a moat
    no closed competitor can copy.
    ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
    Gap: Community/contributor surface
    Why it matters: Both competitors have large contributor bases.
    Catch-up move: The ~30-crate workspace is clean but intimidating. Crate-local AGENTS.md files help; add a good good-first-issue pipeline and
    architecture tour doc for newcomers.
    The strategic summary: don't out-Claude Claude Code. VT Code's defensible position is open + provider-neutral + verifiable: the only terminal agent where
    the harness, event stream, evals, and safety model are all inspectable and run against any model, including local ones. Double down on evals-as-marketing
    , local-model excellence, and protocol interop — those are things Claude Code and Codex structurally cannot offer.

---

# VT Code vs Codex CLI vs Claude Code — Research Note

## Codex CLI Architecture (source: DeepWiki openai/codex — fetched; Zylos Research, 2026-03-26)

- **100+ crate Rust monorepo** (`codex-rs`) — busybox-style binary dispatch, JS distribution shim, dual build (Cargo + Bazel). Key crates: `codex-core`, `codex-tui`, `codex-exec`, `codex-cli`, `codex-app-server`, `codex-mcp`, `codex-config`, `codex-cloud-tasks`.
- **5 execution modes**, all converging on one `ThreadManager`:
  | Mode | Entry | Use case | Persistence | Interaction |
  |---|---|---|---|---|
  | TUI | `codex` | Interactive dev | rollout files | Full UI |
  | Exec | `codex exec` | Automation/CI | rollout (or ephemeral) | Non-interactive |
  | App Server | `codex app-server` | IDE integration | yes | JSON-RPC |
  | MCP Server | `codex mcp-server` | Tool delegation | yes | MCP stdio |
  | Cloud | `codex cloud` | Remote tasks | remote | TUI/CLI for Cloud |
- **Core layering**: `ThreadManager` (thread lifecycle + state DB) → `CodexThread` (submission queue, event emission) → `Session` (turn orchestration, prompt building, streaming) → `ContextManager` (history, compaction, token budget) → `ModelClient` (API retries, SSE parsing) → `RolloutRecorder` (event persistence).
- **Session persistence = rollout files**: serialized event streams on disk (separate active/archived dirs), incremental replay for resumption, offline analysis via `rollout-trace`.
- **Layered config**: defaults → global file → project file → env vars → feature flags → profiles → CLI args (highest precedence), consolidated by `ConfigBuilder`.
- **Sandbox (crown jewel)**:
    - Linux: Bubblewrap — `--unshare-user/pid/net`, read-only `/` root, explicit writable binds, `.git`/`.codex` re-bound read-only, seccomp + `PR_SET_NO_NEW_PRIVS`, fresh `/proc`.
    - Managed proxy mode: TCP→UDS→TCP bridge; seccomp blocks new AF_UNIX/socketpair — prevents sandbox escape via Unix sockets.
    - macOS: Seatbelt layered policy files. Windows: restricted tokens on private desktop (`Winsta0\Default`).
- **Exec policy DSL**: `~/.codex/rules/*.rules` + workspace `.codex/rules/*.rules`; hardcoded-banned: shell interpreters (`python`, `node -e`, `bash -c`, `sudo`, bare `git`).
- **Sandbox modes**: workspace-write (default) → read-only → full-disk-write-access → danger-full-access.
- **Persistent threads**: rollout files + state DB; fork/rollback/archive (Zylos: SQLite-backed).
- **Memory**: two-phase AI pipeline — cheap model extracts from up to 5,000 threads (concurrency 8), strong model consolidates under global lock; stored in `memory_summary.md`, 5k-token cap, SQLite job leases (1h expiry).
- **App-server**: JSON-RPC 2.0 over stdio (NDJSON)/WebSocket — thread lifecycle (start/resume/fork/rollback/compact), steer/interrupt, filesystem RPCs, MCP mgmt, plugin install, TS/JSON-Schema export, Python SDK. Powers VS Code extension.
- **Multi-agent**: `spawn_agent`/`send_input`/`wait_agent`/`close_agent`; depth 1, max 6 parallel; roles (awaiter, explorer); config inheritance.
- **Hooks**: session_start, pre/post_tool_use, stop, `user_prompt_submit` (v0.116.0 — prompt augmentation/filtering).
- **apply_patch**: parsed via formal Lark grammar (structured validation, not string replacement).
- **Model lock**: Responses API only (`wire_api = "chat"` removed — hard error). Default `gpt-5.3-codex` (272k ctx); `gpt-5.1-codex-mini` for background tasks.
- **Velocity**: v0.115–0.117 within 10 days (Mar 2026); 10–15 commits/day, 4–5 core engineers.

## Comparison

| Dimension           | VT Code                                                          | Codex CLI                                                                      | Claude Code                      |
| ------------------- | ---------------------------------------------------------------- | ------------------------------------------------------------------------------ | -------------------------------- |
| Model freedom       | **31 providers + local Ollama/LM Studio/llama.cpp**              | OpenAI Responses API only                                                      | Anthropic only                   |
| Sandbox             | Sandboxed shell, policies, fail-closed adversarial coverage      | **Deeper**: bwrap/Seatbelt/Windows isolation, exec-policy DSL, banned commands | Seatbelt-lite + approval dialogs |
| Session persistence | `ThreadEvent` log, checkpoints, compaction, resume               | SQLite threads, fork/rollback                                                  | In-session only                  |
| Memory              | Per-session state store, cross-session query                     | **AI extraction+consolidation pipeline**                                       | None built-in                    |
| Protocols           | **MCP, ACP, A2A, Skills, Plugins, WebMCP, Open Responses, ATIF** | MCP, app-server, plugins                                                       | MCP, SDK, plugins                |
| Multi-agent         | Subagents, propose/verify, worktrees                             | Structured lifecycle tools, depth limits                                       | Task tool                        |
| Eval                | **`vtcode-eval` pass@k/pass^k, env-based verification**          | Internal                                                                       | Internal                         |
| Distribution        | Open source, niche                                               | OpenAI brand, huge adoption                                                    | Anthropic brand, dominant        |

## Verdict

- **VT Code wins on**: model neutrality (Codex is wire-format-locked), protocol breadth (A2A + WebMCP unique), eval infrastructure as first-class crate.
- **Codex is ahead on**: sandbox depth (namespace isolation + seccomp socket-blocking + managed proxy), automated memory pipeline, unified JSON-RPC app-server, velocity/distribution.

## Catch-up plan (prioritized)

1. **Sandbox to Codex level**: namespace isolation on Linux (bubblewrap or Landlock+seccomp), read-only root + explicit writable binds, protected `.git`/`.vtcode`, managed network proxy mode; adversarial regression suite.
2. **Automated memory**: two-phase pipeline over `vtcode-memory` event log — cheap-model extraction at startup, strong-model consolidation under lock.
3. **Unified programmatic surface**: extend ACP or add JSON-RPC app-server shared by IDE/SDK/TUI; export schema.
4. **Openness wedge**: publish reproducible `vtcode-eval` benchmarks — "same harness, 5 models" (a story competitors can't tell).
5. **Onboarding polish**: one-command install → provider OAuth (`vtcode-auth`) → first task; VS Code extension over app-server.

**Strategic point**: Codex is converging toward where VT Code already is (Rust, formal sandbox, persistent threads, plugins) while staying model-locked. Close sandbox/memory gaps, then lean on provider neutrality + open verifiable evaluation — the axis neither competitor can follow.

## Sources

- https://deepwiki.com/openai/codex (fetched via direct HTTP; overview page)
- https://zylos.ai/research/2026-03-26-openai-codex-cli-architecture-multi-runtime-patterns/
- https://github.com/openai/codex/tree/main/codex-rs

---

Check and fix reverse history search, and with up/down keyboard navigation to traverse command history. when selecting an entry, it only shown 1 line and missing the full command content. Ensure multi-line commands are displayed correctly and navigation works seamlessly.
