open source vtcode-diff, IMPORTANT remember to attribute OpenAI Codex https://github.com/openai/codex as the source of the underlying code and ideas.

===

Improve cross-turn exec-session resume (Plan item D — long-output tool-call follow-up, complements the shipped control-plane budget exemption):

Context: with wait/inspect budget-exempt and the single-call long run (yield_time_ms > 10000) shipped, the remaining gap is turn boundaries. When a turn ends while a `run-*` exec session is still in progress (e.g. check-dev.sh hit the wall-clock or turn limit mid-build), the next turn starts blind: the model must remember the session_id, and a resumed/compacted session may not have it at all.

Status: SHIPPED (this session). Implementation: (1) `ExecSessionManager::in_progress_exec_sessions(cap)` queries the live registry with backend-checked completion (exited sessions filtered); (2) `build_exec_session_resume_note` in `session_loop_runner/support.rs` builds a bounded (<1 KiB) system message carrying session id, command, elapsed time, and a pre-filled `write_stdin` wait shape; (3) injected via `append_transient_turn_notes` at every turn start — this single site covers both the normal next-turn case and session restore/resume (both flow through the turn loop), which is the compaction-safety layer; (4) `SnapshotTurnDiagnostics.in_progress_exec_sessions` (serde-default Vec, bounded to 4) records the ids at turn end for diagnostics/ATIF correlation. Tests: registry filter/cap test, note-builder test (identity + wait args + bounded size + cleared-after-close), snapshot round-trip. Remaining follow-up: if the injected message shape becomes part of the runtime contract, update `docs/harness/` + prompt golden tests.

===

debug session and fix: "[!] Turn balancer: repeated low-signal navigation detected (exec::inspection:
:rg::exit ×5); scheduling an early recovery pass."

===

double check compaction /compact if it works properly. "Compaction failed: Failed to generate compaction summary", check last session and debug and fix.

---

check and fix " Blocked action
exec_command is denied by workspace tool policy, so I could not run the
final extraction:
• Denial: "Tool 'exec_command' execution denied by policy";"

===

help me check cache hit rate is too low. 8.3% hit rate

Session 6m 15s | 431.9k in / 12.1k out | Cache 35.8k read (8.3% hit rate) | Code +37 / -0

==> check session 20260916T033857Z_346951-33962 and fix for vtcode, not do it yourself
/Users/vinhnguyenxuan/Developer/learn-by-doing/vtcode/.vtcode/sessions/session-vtcode-20260916T033857Z_346951-33962 /Users/vinhnguyenxuan/Developer/learn-by-doing/vtcode/.vtcode/tasks/blockers/session-vtcode-20260916t033857z_346951-33962-20260916T034301Z-fcb0f192-9439-42a4-8fe5-4c3708aa9ac1.md
==> also self-check and fix for all other providers, not just openai. use web search for research docs and apply to improve VT COde

===

check prompt caching issues and doesn't work: "Prompt cache suffered 3 consecutive near-full misses; recent requests re-paid full input cost. Check for
prompt/tool-catalog churn (model switches, MCP refreshes, planning toggles) or long idle gaps expiring the provider
cache."
