reference compaction summarization template for auto compact -> continuation

```
--- compaction ---

+ Thought: 9.5s
Objective
Restructure vscode-extension/, zed-extension/, and xtask/ into better locations and update all codebase references. Then continue with build improvements, starting with fixing failing tests caused by broken docs/models.json path in vtcode-config build script after the restructure.
Important Details
User chose: both extensions → extensions/; xtask → crates/codegen/xtask/
cargo check --locked passes after all moves
cargo nextest run --profile quick revealed 3 test failures in model picker/capabilities:
static_model_search_terms_include_modalities_and_tool_state expects "modalities" but gets "input text image"
test_generated_model_capability_lookup panics at model_catalog_entry("openai", "gpt-5.4") (returns None)
test_enum_variants_match_all_models_collection enum mismatch
Root cause: vtcode-config/build.rs generates model_capabilities.rs from docs/models.json. After moving crates under crates/codegen/, the hardcoded ../docs/models.json path is wrong. Generated file shows empty ENTRIES/PROVIDERS, meaning load_model_capability_entries is returning Ok(Vec::new()) because the file isn't found.
Path math from crates/codegen/vtcode-config/: ../../../docs/models.json reaches repo-root docs/models.json. The load_provider_metadata function was already updated to this path; load_model_capability_entries still has the wrong relative path (../../docs/models.json).
sccache was masking build script execution during earlier rebuilds; bypass with RUSTC_WRAPPER="" cargo clean -p vtcode-config && RUSTC_WRAPPER="" cargo build -p vtcode-config
Work State
Completed
Moved vscode-extension/ → extensions/vscode-extension/
Moved zed-extension/ → extensions/zed-extension/
Moved xtask/ → crates/codegen/xtask/
Updated all references in Cargo.toml, docs, scripts, continuation.rs, root AGENTS.md, crate AGENTS.md files
Verified cargo check --locked passes
Identified failing tests and root cause in vtcode-config/build.rs
Active
Fixing crates/codegen/vtcode-config/build.rs path so model capabilities are actually generated from docs/models.json
Need to verify generated model_capabilities.rs contains real data
Need to re-run failing tests to confirm fixes
Blocked
None currently, but path fix in load_model_capability_entries is incomplete and debug logging may still be present in build.rs
Next Move
Fix crates/codegen/vtcode-config/build.rs: change load_model_capability_entries path from ../../docs/models.json to ../../../docs/models.json and remove any debug eprintln! statements
Rebuild with RUSTC_WRAPPER="" cargo clean -p vtcode-config && RUSTC_WRAPPER="" cargo build -p vtcode-config and inspect generated target/debug/build/vtcode-config-*/out/model_capabilities.rs to confirm non-empty ENTRIES/PROVIDERS
Run the failing tests: cargo nextest run -p vtcode agent::runloop::model_picker::tests::static_model_search_terms_include_modalities_and_tool_state and cargo nextest run -p vtcode-config models::tests::test_generated_model_capability_lookup
Continue with broader performance/IO deep-dive using the vtcode-dev-helper skill (quality gate, clippy, nextest) after model capability tests pass
Relevant Files
/Users/vinhnguyenxuan/Developer/learn-by-doing/vtcode/crates/codegen/vtcode-config/build.rs — build script with broken relative path to docs/models.json causing empty model catalog generation
/Users/vinhnguyenxuan/Developer/learn-by-doing/vtcode/docs/models.json — source of truth for model capabilities
/Users/vinhnguyenxuan/Developer/learn-by-doing/vtcode/src/agent/runloop/model_picker/tests.rs — failing test expecting "modalities" term
/Users/vinhnguyenxuan/Developer/learn-by-doing/vtcode/crates/codegen/vtcode-config/src/models/tests.rs — failing capability lookup test
/Users/vinhnguyenxuan/Developer/learn-by-doing/vtcode/.agents/skills/vtcode-dev-helper — skill for quality gate, testing, linting workflows
▣  Compaction · Step 3.7 Flash · 26.6s

// then agent continue and resume works based on the above context, and the agent will continue to work on the next steps of the project, including fixing the build script path and verifying that the model capabilities are generated correctly. The agent will also run the failing tests to ensure that the fixes have resolved the issues.
```

===

===

reference and explore research /Users/vinhnguyenxuan/Developer/learn-by-doing/claude-code-main and apply learning to improve vtcode codebase.

===

Final Plan: grok-build → vtcode Improvements
Key Corrected Understanding
Tool taxonomy — vtcode already has sophisticated deferred loading (DeferredToolPolicy for Anthropic/OpenAI/client-local), token-budget gating (15-tool threshold, 4000-token budget), and ToolDocumentationMode::Minimal/Progressive/Full. Adding ToolKind enum provides cross-toolset consistency but is not a token-efficiency win.
Compaction — vtcode already has hierarchical summaries (abstract/detail/verbatim), memory envelopes, provider-native compaction fallback, and unified orchestrator used by both runloops. Two-pass is a genuine improvement but the marginal gain is smaller than against a naive system.
Hooks fail-open — This is a real and significant gap. grok-build's dispatcher.rs:20-27 explicitly documents fail-open for pre_tool_use: timeouts, crashes, command-not-found, and malformed output do NOT block the tool call. vtcode's interpret/tool.rs:17-26 does the opposite: a timeout converts Continue to Deny. This is a reliability and UX issue.
Goal tracking — vtcode has ProgressMonitor + ProgressLedger with stall detection, but it's a flat ledger, not a state machine. grok-build's GoalTracker has 7 distinct pause states (UserPaused, BackOffPaused, NoProgressPaused, InfraPaused, Blocked, BudgetLimited, Complete) with explicit forward-compat deserialization.
Memory — vtcode has MarkdownLoopMemory + SqliteLoopMemory (behind sqlite feature flag) for loop-level notes/decisions. But there's no semantic retrieval — no vector search, no MMR, no temporal decay. grok-build's memory system is a full hybrid search engine.
Final Priority-Ordered Plan
Phase 1: IO & Stability (Week 1-2) — Low risk, immediate measurable value
1.1 Buffered writes + turn-boundary flush in session store

- File: crates/codegen/vtcode-session-store/src/event_log.rs
- Change: Wrap events.jsonl in BufWriter (8KB buffer). Flush on TurnCompleted/TurnFailed, not per-event.
- Impact: ~100x reduction in syscalls for bursty event streams.
- Risk: Low. BufWriter flushes on drop; turn-boundary flush preserves crash safety.
  1.2 Turn-index manifest for O(1) session open
- File: crates/codegen/vtcode-session-store/src/event_log.rs + new manifest.rs
- Change: Write manifest.json on every turn boundary with session_id, turn_count, event_count, last_offset, status, created_at. On open, read manifest (O(1)), skip scan() unless missing/stale.
- Impact: Eliminates O(n) JSONL parse on session open. For a 10K-event session, startup drops from ~200ms to <1ms.
- Risk: Low. scan() remains authoritative rebuild path.
  1.3 TTY detach utility
- File: crates/codegen/vtcode-bash-runner/src/tty.rs
- Change: Port detach_from_tty() — setsid() + setpgid(0,0) EPERM fallback. Call before spawning interactive subprocesses.
- Impact: Prevents vim/less/top from hijacking the TUI.
- Risk: Low. Well-understood POSIX semantics.
  1.4 Interjection buffer primitive
- File: New crates/common/vtcode-interjection-core/src/{buffer,events,format}.rs
- Change: InterjectionBuffer for structured user interruptions mid-turn. Enables cancel-in-progress tool calls, inject urgent queries, handle CTRL+C gracefully.
- Impact: Cancellation UX improvement; foundation for mid-turn steering.
- Risk: Low. Clean separation of concern.
  Phase 2: Architecture & Extensibility (Week 3-4) — Low-medium risk, compounding value
  2.1 StringInterner for file indexer
- File: crates/codegen/vtcode-indexer/src/lib.rs
- Change: Port grok-build's StringInterner — FxHash + arena Vec<u8> + SmallVec collision list. Segment paths, intern common prefixes.
- Impact: 60-80% memory reduction for path-heavy workloads. O(1) intern/get_id, O(k) path reconstruction.
- Risk: Low. Internal representation change; external API unchanged.
  2.2 ToolPack process-global registry
- File: crates/codegen/vtcode-core/src/tools/registry/builtins.rs
- Change: Add static TOOL_PACKS: OnceLock<Mutex<Vec<ToolPack>>>. Make register::<T>() public for out-of-tree packs.
- Impact: vtcode-mcp, vtcode-skills self-register at startup. vtcode-core doesn't need to know about them.
- Risk: Low. Pure architectural inversion; no behavior change.
  2.3 Fail-open hooks semantics
- File: crates/codegen/vtcode-core/src/hooks/lifecycle/interpret/tool.rs
- Change: In interpret_pre_tool, remove the timeout→deny conversion (lines 17-26). Instead: timeout → log warning + Continue. Only explicit deny blocks.
- Impact: Hook failures (timeouts, crashes, config errors) no longer block innocent tool calls. Matches grok-build's protected-environment threat model.
- Risk: Medium. Changes security semantics — must be documented and configurable.
  Phase 3: Verification & Memory (Week 5-7) — Medium-high risk, high impact
  3.1 Adversarial skeptic panel for verification
- File: crates/codegen/vtcode-core/src/subagents/verifier.rs
- Change: Add N-way parallel subagent verification with majority-refute aggregation. Replace single-verifier path in controller_verify.rs. Fail-open with stall early-exit (consecutive identical gap fingerprints).
- Impact: Catches false positives in "is this goal done?" checks. Reduces premature completion.
- Risk: Medium. Subagent spawn overhead; needs cost caps. Should live alongside existing single-verifier path behind a config flag.
  3.2 Memory hybrid search (new crate)
- File: New crates/codegen/vtcode-memory/src/{backend,index,search,embedding,mmr}.rs
- Change: FTS5 + optional sqlite-vec KNN + MMR diversity + temporal decay. Index .vtcode/context/tool_outputs/ and history/.
- Impact: Semantic cross-session recall ("what did we decide about auth last week?").
- Risk: High. New dependency (sqlite-vec), new configuration, embedding API cost.
  3.3 Goal tracker state machine
- File: crates/codegen/vtcode-core/src/core/agent/goal_tracker.rs
- Change: Replace ProgressLedger with GoalTracker having states: Active, UserPaused, BackOffPaused, NoProgressPaused, InfraPaused, Blocked, BudgetLimited, Complete. Add GoalPauseReason enum with forward-compat deserialization.
- Impact: Distinguishes "verifier sees same gaps" from "run cap hit" for better UX.
- Risk: Medium. Must preserve ProgressLedger persistence format for backward compat.
  Phase 4: Advanced (Week 8+) — Higher effort, strategic value
  4.1 Two-pass compaction
- File: crates/codegen/vtcode-core/src/compaction/
- Change: Add split_index_by_token_fraction + snap_split_idx_to_tool_boundaries. Pass1: summarize 95% → NOTE₁ (capped 12K chars). Pass2: rewrite NOTE₁ + 5% tail → NOTE₂.
- Impact: Reduces prefill for compaction from ~100% to ~5% + summary size.
- Risk: Medium. Must snap to tool-call boundaries to avoid splitting tool_calls from ToolResult.
  4.2 ToolKind taxonomy (lower priority)
- File: crates/common/vtcode-utility-tool-specs/src/
- Change: Add ToolKind enum (Read, Edit, Execute, Search, Lsp, Task, WebSearch, WebFetch, Background, Skill, Memory, Goal, Other). Add ToolNamespace enum. Add CanonicalToolMeta with \_meta envelope.
- Impact: Cross-toolset consistency, canonical labeling, foundation for future capability improvements. NOT a token-efficiency play.
- Risk: Low but scope is broad (every tool registration site).
  What NOT To Do (and why)
  Excluded Idea Reason
  SQLite session store vtcode uses JSONL successfully; the manifest (1.2) addresses the real IO cost. SQLite is a large dependency change for NFS resilience that affects few users.
  Container sandboxing vtcode-safety already has seccomp/seatbelt + policy layers. Container isolation is a different threat model.
  LSP integration vtcode-indexer is file-based; LSP is a separate project.
  Incremental file indexing StringInterner (2.1) addresses the memory bottleneck more directly; incremental indexing adds complexity for marginal IO gain.
  Execution Order with Dependencies
  Phase 1 (independent, safe):
  1.1 → 1.2 → 1.3 → 1.4
  (all can be done in parallel; no dependencies between them)

Phase 2 (architecture):
2.1 → 2.2 → 2.3
(2.2 enables 2.3 if 2.3 uses ToolPack registration)

Phase 3 (strategic):
3.1 → 3.2 → 3.3
(3.3 can happen anytime; lowest priority)

Phase 4 (advanced):
4.1 → 4.2
Questions Before Proceeding

1. Should 1.1 + 1.2 be combined into a single "session store IO" PR, or kept separate? They touch the same file but solve different problems.
2. For 3.1 (skeptic panel), should it replace controller_verify.rs entirely or live alongside it behind a config flag? The existing single-verifier path is simpler and cheaper.
3. For 3.2 (memory search), should it be a new crate (vtcode-memory) or extend vtcode-session-store? A new crate keeps concerns separated but adds workspace complexity.
4. For 2.3 (fail-open hooks), should this be the default or opt-in via config? Changing security semantics is controversial.
5. Should I start implementing now, or do you want to adjust priorities first?

===

1.  Should 1.1 + 1.2 be combined into a single "session store IO" PR, or kept separate? They touch the same file but solve different problems. => your recommendation
2.  For 3.1 (skeptic panel), should it replace controller_verify.rs entirely or live alongside it behind a config flag? The existing single-verifier path is simpler and cheaper. => your recommendation
3.  For 3.2 (memory search), should it be a new crate (vtcode-memory) or extend vtcode-session-store? => maybe extend vtcode-session-store. and rename vtcode-session-store to vtcode-memory => update workspace ref
4.  For 2.3 (fail-open hooks), should this be the default or opt-in via config? => your recommendation
