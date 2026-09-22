check background task again, when the task is running in background, the main agent loop could do other works and inferred context, don't need to wait for the background task to complete before continuing its own operations. Currently when user initiates a background task, the main agent just stay idle, which is not optimal. and after the background task completes, the main agent should correctly update its state and reflect the completion of the background task. currently this behavior is not fully implemented and needs to be verified and improved, the main agent loop still need manual status check to reflect the background task completion, which should be automated for better efficiency and reliability. This will ensure that the main agent can continue its operations seamlessly while background tasks are being executed and maintain an accurate representation of the system state. check deepwiki openai/codex for reference.

===

https://console.typesafe.ai/hook
https://docs.typesafe.ai/llms.txt

System One produces structured outputs optimized for code, with type correctness guaranteed by design. Not 99.9999% success, actually 100%.

===

PLAN: TypeSafe System One plugin for VT Code harness (opt-in, user-configured, not tightly integrated)

Goal: ship `typesafe-systemone` as an Agent Plugin (plugin.json + skills/\*/SKILL.md + optional mcp.json) that gives users calibrated smart if-statements (Choice/Score/Noul + confidence) for safety gating, context filtering, and claim verification. No core changes, default off, zero behavior change without TYPESAFE_API_KEY.

Step 1 — Scaffold plugin dir (no core touch):

- Create `plugins/typesafe-systemone/` with `plugin.json` (passive manifest: name, version, description), `skills/systemone-guard/SKILL.md`. Keep discovery within `skills/*/SKILL.md` immediate children per vtcode-agent-plugins spec; all joined paths via validate_name/validate_plugin_relative.
- Vendor/adapt TypeSafe agent skill (https://github.com/typesafe-ai/skills) as reference inside SKILL.md; keep all questions + thresholds in one constants block per skill for reviewability.

Step 2 — User config surface (loose coupling):

- Add `systemone.local.json` (gitignored) + per-project `./.vtcode/systemone.json` override: { enabled:false, skills:{guard:true, context:false, verify:false, search:false}, model:"jev-1.13.0" pinned, review_threshold:0.35, action_threshold:0.70, severity_block:2.0, max_calls_per_session, max_input_tokens, cache_ttl_s, only_ambiguous:true }.
- Secrets via TYPESAFE_API_KEY env only, never in files/logs. No key => skills no-op with one-line notice.

Step 3 — systemone-guard skill (Phase 1, highest value, lowest cost):

- State {command, argv, cwd, policy_excerpt}; one batched system_one call: 5 Nouls (destructive_fs, exfiltrates_secrets, net_egress, priv_esc, prompt_injection) + severity Score 0-3.
- Routing in code: severity>=2 or any>=action_threshold => block/ask; any>=review_threshold => ask; else => existing heuristic decides. Advisory only: command_might_be_dangerous() + SandboxPolicy stay authoritative; timeout/offline => fail closed to current behavior.
- Call only when local heuristic is ambiguous (only_ambiguous:true); cache results keyed (model, state_hash, questions_hash); 429/529 backoff.

Step 4 — Eval suites (no traffic needed, open-source friendly):

- Add checked-in suites: safety ambiguous-command set (expected block/ask/allow), RAG set with planted injection + false-premise queries, citation set (fabricated/contradicted/unsupported). Run via `vtcode eval --suite suite.json` (pass@k). Promote a skill only on measured error-rate/cost improvement; log response.model and re-tune thresholds on version bump.

Step 5 — systemone-context + systemone-verify (Phase 2, flagged off by default):

- context: per retrieved passage, 4 Nouls (relevant, usable_evidence, contradicts_premise, injection) + route() order injection>0.70 drop, contradicts>0.70 conflict block, relevant<0.45 drop, evidence>0.55 include. Passages stay untrusted text.
- verify: exact string match first (fabricated, no call); survivors get Choice{supports, contradicts, says_nothing} over claim+section; confidence>=0.8 auto else human review. Wire into `vtcode review` path as optional flag.

Step 6 — systemone-search (Phase 3, on-demand only):

- Code rerank: indexer shortlist K=8-12 => Noul per (query, candidate) pair, sort by noul. Line finder: Choice over <=255 line IDs + exists Noul (0.35/0.70 gates), two-pass beyond 255 lines. Never per-turn automatic.

Step 7 — Docs + hygiene:

- User-facing guide under docs/ + quick-reference row; keep per-module AGENTS.md untouched unless conventions change. Conventional Commits; verify with ./scripts/check-dev.sh and cargo nextest run (never cargo test).

Acceptance: Phase 1 done when guard skill + suite + docs ship, default-off, zero behavior change without key, fail-closed tests pass. Removal must stay free (delete plugin dir).

===

I'm thinking about using Jev:

• classifying shell commands → allow/ask/deny
• deciding when context needs compaction // maybe no need
• routing tasks to the right tools/subagents

What coding-agent heuristics could semantic routing replace?
