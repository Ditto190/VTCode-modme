Improve apply/edit patch UI

1. Change the apply/edit patch UI to be more intuitive.
2. style and syntax highlight the file and diff color on the edit patch
3. reason code kept stable; renderers show `diff_preview_user_message`
4. Improve and streamline the overall UI of file ops UI/UX in the TUI
5. Follow design system

current it look blank and seems unintuitive and duplicated '/Users/vinhnguyenxuan/Documents/vtcode-resources/bugs/Screenshot 2026-09-15 at 16.53.19.png'

===

improve git diff preview UI '/Users/vinhnguyenxuan/Documents/vtcode-resources/idea/Screenshot 2026-09-14 at 14.26.27.png' it too dimmed.

====

open source vtcode-diff

===

Implement keyboard shortcut enhancements in chat input text field:

1. command (macos:) or similar key on other platforms: a/ command+backspace should clear all text input in a single line of multi-line chat input. if it is single-line chat input, it should clear the entire input. also applied for compact [pasted content ... chars] block and image block.
2. command+left or command+right should move the cursor to the beginning or end of the current line in multi-line chat input. if it is single-line chat input, it should move the cursor to the beginning or end of the entire input.
3. if the cursor focus is active in chat input, pressing the double-escape key should clear the current line in multi-line chat input or the entire input in single-line chat input.
4. if the cursor focus is active in chat input, pressing tab should enqueue the messages to queue list (similiar to control+enter).

===

reduce plan mode approval blank gap height '/Users/vinhnguyenxuan/Documents/vtcode-resources/idea/Screenshot 2026-09-14 at 15.57.49.png'

also remove the leading bullet "•" dots in the plan mode approval header section.

Also, idea, in the plan mode approval modal, add a summarized section that provides an overview of the changes or actions to be approved => this will help users quickly understand what they are approving without having to go through all the details. Can add the summary paragraph from the proposed plan and display it in this section. keep it short and concise and fit modal text lenght limit.

===

CRITICAL: check the message queues system. currently when it picking messages from the queue, it only use the last message and ignores the rest and the queue get cleared. This is CRITICAL and needs immediate attention.

===

idea: improve both on the user experience and the VT Code harness system itself. when hitting:

```
[!] Anti-Blind-Editing: run a verifier (build/test/lint — e.g. `cargo check`,
`go test`, or `pytest`) and let it exit 0 before further edits.
• The turn is blocked because verification is still pending. Inspection-
only checks do not clear the verification gate; run a verification command
— your project's build/test/lint tool, e.g. cargo check --locked, go test,
or cargo nextest run (standalone or as a pure && chain, no | head pipes and
no ;/||/| joins) — to exit 0, then resume the request. A failed verifier
grants 2 fix-up edits before re-verify is required.
Turn blocked after repeated unverified assistant responses; verification is
still pending.
Turn blocked: Turn blocked after repeated unverified assistant responses;
verification is still pending.
What you can do:
• In this session: Type 'continue' to resume, or describe alternative
instructions
• From terminal: Run `vtcode --resume session-vtcode-
    20260914T084315Z_111294-82729`
• Blocker details: /Users/vinhnguyenxuan/Developer/learn-by-doing/vtcode/.
vtcode/tasks/current_blocked.md
• Archived details: /Users/vinhnguyenxuan/Developer/learn-by-doing/vtcode/.
vtcode/tasks/blockers/session-vtcode-20260914t084315z_111294-82729-
20260914T091125Z-3f8578c8-e1da-44ca-8158-543d86056f32.md
Repeated follow-up after stalled turn detected; enforcing autonomous recovery
and conclusion.
```

====> without user having to manually intervene and ensuring the verification process is properly handled. and not blocking long-running tasks. THIS IS CRITICAL. Find a way to automatically manage the verification gate and recovery from stalled turns with actionable steps to help vtcode agent continue its operation smoothly. you can research on deepwiki mcp for how openai/codex handles similar scenarios.

===

help me check and fix on control+g to edit/review plan proposal in external editor. on pressing, the external editor should open with the current plan proposal loaded, allowing the user to make changes and save them. after saving and closing the editor, the changes should be reflected back in the vtcode interface seamlessly. Bug: currently it does not open the plan file in external editor and also the modal approval is disappeared.

===

check VT Code notification from terminal/ghostty etc doesn't showing in the notification center properly. Bug: notifications are not appearing as expected, making it difficult for the user to stay informed about important events and updates. Currently, only the terminal get pinged, and the notification center remains empty and I am missing critical notifications. (Note: currently tested on macOS and ghostty, other platforms may have similar issues.). Use sound alerts and system notifications to ensure the user is properly informed about important events and updates. Configurable by the user through the settings.

===

Change keyboard shortcut to switch modes/agents from single tab to shift+tab.

===

Help me check this session run log and harness atif trajectory. Even though it very stable and good now, more than I expected, I want to ensure that there are no hidden issues or potential improvements that can be made. Check for potential optimizations, edge cases, and any anomalies that might affect long-term stability and performance. Eg: memory leaks, race conditions, unexpected errors, performance bottlenecks, and unusual patterns in the logs, context and tool calls optimzation that might need attention and can further improve the system. Also check if VT Code prompt caching is functioning correctly and efficiently. check for cache hit / miss ratio and any potential improvements in caching strategy. Make sure cost efficiency is maintained and any unnecessary resource usage is minimized.

```
> VT Code (0.162.2)
Safe tools
Model: zai/glm-5.3-flash via Merge Gateway · high
Session 1h 9m 14s | 23.5m in / 80.3k out | Code +6 / -0
Resume: vtcode --resume session-vtcode-20260914T084315Z_111294-82729
```

log run 1:

```
session-vtcode-20260914T084315Z_111294-82729.
Path: /Users/vinhnguyenxuan/Developer/learn-by-doing/vtcode/.vtcode/sessions/session-vtcode-20260914T084315Z_111294-82729
/Users/vinhnguyenxuan/Developer/learn-by-doing/vtcode/.vtcode/tasks/blockers/session-vtcode-20260914t084315z_111294-82729-20260914T091125Z-3f8578c8-e1da-44ca-8158-543d86056f32.md
```

and

run 2:

```
/Users/vinhnguyenxuan/Developer/learn-by-doing/vtcode/.vtcode/sessions/session-vtcode-20260914T085310Z_717264-90834 /Users/vinhnguyenxuan/Developer/learn-by-doing/vtcode/.vtcode/tasks/blockers/session-vtcode-20260914t085310z_717264-90834-20260914T101118Z-9171e298-af42-47c0-91bb-5df2456fe1c5.md /Users/vinhnguyenxuan/Developer/learn-by-doing/vtcode/.vtcode/tasks/blockers/session-vtcode-20260914t085310z_717264-90834-20260914T100930Z-260d9d55-4447-4e34-992a-b10ad6830912.md
```

use relevant session logs and task blockers to analyze the system's behavior and identify potential improvements. use skills and distil insights from the logs to make informed recommendations for enhancing stability, performance, and overall efficiency.

===

IMPROVE todo task list, make it shorter and more concise in the TUI. only need details internally to the harness. Keep the TUI clean and focused on essential information for the user. also improve the UI/UX of the TODO/task, replace □ with more intuitive UI and color coding for different task statuses.

reference this UI: /Users/vinhnguyenxuan/Documents/vtcode-resources/idea/G_V_oL6W4AAnHSU.jpg. but adapt it to fit the TUI constraints and ensure it remains user-friendly and visually clear and use VT Code principles for consistency.

===

audit and double check and self test all TUI's commands for up to date and correctness and ensure they function as expected in various scenarios.

===

remove commands: /suggest, /advisor, /command. merge /agents with /agent.

---

add /feedback to open new issues on github: https://github.com/vinhnx/VTCode/issues/new/choose

---

quality of life improvement idea: when the plan mode Turn blocked, Mutation blocked => suggest or ask user to switch to build/auto modes(agents) to use the system effectively without being hindered by blocked states and blocked permissions. also, try to trigger user HITL popup to let user choose and switch modes or stay at the current plan mode. explain the reasoning behind the suggestion and provide clear guidance to the user.

---

help me improve for long output stdio tool call. the vtcode harness often hits the tool-call limit, causing incomplete execution of scripts and tests.

```
•   The turn hit the tool-call limit while
./scripts/check-dev.sh was still running (session run-
2b7098be). The check may still be running in the
background — I'll pick it up next turn.
  Status:
  • All plan steps (1–4) implemented and verified: cargo
    check -p vtcode-core ✅ , cargo nextest run -p vtcode
    -core -E 'test(analyze)' ✅  (5/5 tests pass)
  • Two follow-up fixes from the smoke test applied:
    max_items: 200 (root cause of the 20-entry cap) and
    the duplicated Detected literal removed
  • ./scripts/check-dev.sh was re-run after those fixes
    but the turn ended before it completed
  Next turn: confirm check-dev.sh result, re-run the
smoke test, then commit as requested.
  Tool budget for this turn is exhausted — I can't run
./scripts/check-dev.sh right now. All code changes are
complete and previously verified (cargo check ✅ , cargo
nextest run -p vtcode-core -E 'test(analyze)' ✅  5/5).
The remaining step is re-running the fast gate after the
last two small fixes (max_items: 200, Detected literal),
then the smoke test and commit.
  Please send any message (e.g. "continue") to start a
  fresh turn so I can run the final gate and commit.
```

==> find a solution to handle long output from stdio tool calls more effectively, ensuring that the VTCode harness can continue execution without hitting the tool-call limit prematurely.

===

Help me check this session run log and harness atif trajectory. Even though it very stable and good now, more than I expected, I want to ensure that there are no hidden issues or potential improvements that can be made. Check for potential optimizations, edge cases, and any anomalies that might affect long-term stability and performance. Eg: memory leaks, race conditions, unexpected errors, performance bottlenecks, and unusual patterns in the logs, context and tool calls optimzation that might need attention and can further improve the system. Also check if VT Code prompt caching is functioning correctly and efficiently. check for cache hit / miss ratio and any potential improvements in caching strategy. Make sure cost efficiency is maintained and any unnecessary resource usage is minimized.

```
> VT Code (0.162.2)
Safe tools
Model: zai/glm-5.3-flash via Merge Gateway · high
Session 1h 9m 14s | 23.5m in / 80.3k out | Code +6 / -0
Resume: vtcode --resume session-vtcode-20260914T084315Z_111294-82729
```

log: session-vtcode-20260914T084315Z_111294-82729.
Path: /Users/vinhnguyenxuan/Developer/learn-by-doing/vtcode/.vtcode/sessions/session-vtcode-20260914T084315Z_111294-82729
/Users/vinhnguyenxuan/Developer/learn-by-doing/vtcode/.vtcode/tasks/blockers/session-vtcode-20260914t084315z_111294-82729-20260914T091125Z-3f8578c8-e1da-44ca-8158-543d86056f32.md

use relevant session logs and task blockers to analyze the system's behavior and identify potential improvements. use skills and distil insights from the logs to make informed recommendations for enhancing stability, performance, and overall efficiency.

also check recent log session-vtcode-20260914T085310Z_717264-90834

research best practice and models/providers guide for all supporting in VT Code. then review and deep dive and fix to improve

Review the proposed plan and logs again, check nearby code for correctness, regressions, unintended behavior changes, and unnecessary complexity. Reuse existing patterns where possible. Rank confirmed issues by severity, filter false positives, apply justified DRY/KISS refactors, fix, test, and re-review until clean. Then implement fixes for each one.

deep dive again

plan: Stability / cost audit: 084315Z_111294 + 085310Z_717264
Checked: events.jsonl (2293 + 1220 lines), derived/atif-trajectory.json, manifest.json, blocker handoff, turn.\* usage progression, tool-latency harness events, slow-command join.
Headline: functionally stable, but cost efficiency is broken and long-run stability has 3 load-bearing risks. No memory leak seen (persisted_file_len 1.6MB/1.0MB, bounded), no race in logs — but cache, verification loop, and exec patterns will degrade cost/latency over time.

1. Prompt caching: 0% hit rate — confirmed broken, not “efficient”
   Evidence:

- Every turn.completed/failed/blocked in both sessions: cached_input_tokens: 0, cache_creation_tokens: 0. E.g. sess-084315 turn with input_tokens: 5,050,328, sess-085310 with 2,351,222 — all re-paid.
- ATIF final_metrics: total_prompt_tokens: 25,859,535, completion: 102,459 — ~250:1 in/out ratio, header 23.5m in / 80.3k out.
- Root cause in code:
- crates/codegen/vtcode-llm/src/providers/merge_gateway.rs:1017-1046 parse_native_usage() hard-codes cached_prompt_tokens: None, cache_creation_tokens: None, cache_read_tokens: None. Native /v1/responses path never surfaces cache signal even if gateway returns it.
- merge_gateway.rs:178-217 from_config() drops prompt_cache for native path — only forwarded to legacy_core when base URL ends in /v1/openai. Native build_native_payload() (merge_gateway.rs:355-430) never emits cache directives.
- Result: crates/codegen/vtcode-core/src/core/agent/cache_health.rs:135-138 is_measured() requires cached>0 || creation>0, so PromptCacheHealthMonitor records 0 measured turns and never fires — silent failure by design. The monitor is correct for “unmeasurable provider” but there is no “unmeasurable” warning path.
  Best-practice gap: Anthropic/OpenAI-compatible routes need stable prefix + explicit cache breakpoints; ZAI zai/glm-5.3-flash via Merge Gateway native responses has no documented prompt-cache contract in-repo. Need to check gateway docs for prompt_cache_hit_tokens / prompt_cache_write_tokens (pattern already parsed in openai/responses_api.rs:408, openrouter/stream_decoder.rs:18) and map them in parse_native_usage. If gateway truly doesn’t support caching for zai/\*, the fix is to surface that (picker warning + cache_health unmeasurable alert), not silently report 0.

2. Tool-call / context optimization — biggest token + wall-time sink

- exec_command dominance: 654/812 (80%) in sess-084315, 417/521 (80%) in sess-085310. code_search: 15/27, apply_patch: 98/12, write_stdin: 42/39. Agent shells out for sed -n, rg -n, awk, git diff --stat loops instead of read_file with ranges / code_search / unified_file. Each adds ~230ms + full tool-output tokens to history.
- Repeated heavy builds hitting the 30s ceiling: 37 slow execs >5s in sess-084315 (e.g. cargo check 30045ms, check-dev.sh 30015-30053ms ×6, nextest 60152ms, clippy+fmt 30028ms), sess-085310 has nextest 137068ms, check-dev.sh 111458ms, cargo run analyze 76787ms. Timeout → retry → double-pay. No use of background exec_sessions or ./scripts/check-dev.sh --test/--changed narrowing.
- apply_patch errors (15 + 32 error outputs): invalid hunk header '\*\*\* End Patch' (unified-diff misuse), target already exists, Unknown command: main/performance/nonexistent from man scaffolding loops.
- Preflight friction: Command injection pattern detected, dynamic shell expansion in find commands is not allowed, tool denied by planning workflow ×4, failed to resolve code_search path .../tool_policy (guessed path). Agent fights guardrails and retries.
- Permission latency: permission.resolved wait_ms: 41468, 34639, 9067 — human approval dominates the 1h09m wall time.
- Interjections: 13 in sess-084315 (direct + queue) — prompt churn that would break prefix cache even if caching worked.

3. Verification / planning blocked loop
   Both sessions hit turn.failed: turn blocked → turn.blocked: repeated unverified assistant responses (sess-084315 blocked_total: 2, sess-085310 blocked_total: 1 then 4). Planning recovery also fails: planning recovery did not produce an approval-ready plan. This is the blocker-handoff path that wrote .vtcode/tasks/blockers/...3f8578c8...md. Recovery eventually succeeded (resolved_at 09:23), but the loop burns turns with 0 evidence progress. Needs threshold/guidance review, not just retry.
4. ATIF / harness trajectory anomaly

- Sess-085310 derived/atif-trajectory.json is empty: steps: [], total_prompt_tokens: 0. Sess-084315 has 654 steps but step0 keys: [step_id, timestamp, source, message] — agent-message only, no tool/latency/usage per step. final_metrics.total_steps: 654 matches exec_command count, suggesting steps counted but not materialized. Breaks eval/replay parity (ThreadEvent contract).

5. Context management

- Per-turn input_tokens swings 196k → 5.05M → 4.6M → 1.1M with no context.reset in sess-084315 (only 1 reset in sess-085310). Either history + spool concatenation is unbounded that turn, or usage accounting is cumulative — either way needs verification against effective_context_budget / turn_preview_budget_bytes (32 KiB exec, 96 KiB planning per vtcode-core/AGENTS.md).
  Proposed plan (ranked, for build agent)

1. P0 — Merge Gateway cache signal: inspect live gateway usage payload for zai/glm-5.3-flash, extend parse_native_usage + stream response.usage handling, wire prompt_cache into native payload or document unsupported; add cache_health “unmeasurable provider” one-shot diagnostic so 0/0 stops being silent. Test: cargo nextest run -p vtcode-llm -E 'test(merge_gateway)', new usage-parser unit tests with real gateway sample.
2. P0 — ATIF export: fix empty/impoverished trajectory (tool steps, usage, latency) for both runloops; regression test on the two session fixtures.
3. P1 — Exec economy: prompt guidance to prefer read_file/code_search over sed/rg via exec_command, batch parallel-safe reads, raise/background long cargo cmds (exec_sessions, check-dev.sh --changed/--test), dedupe cargo check/build loops. Validate with ./scripts/check-dev.sh --test + --changed.
4. P1 — Verification gate: review turn.blocked thresholds + “unverified response” classifier; require evidence markers before retry, cap preflight-denied retries (especially planning-denied + security-denied).
5. P2 — Patch + path hygiene: apply_patch schema hint (no unified diff), pre-check target already exists, fix code_search path guessing; permission fast-path for read-only safe tools to cut 40s waits.

===

revamp and rebuild the whole `/config` modal. For better user experience, ensure that all configuration options are easily accessible, clearly documented, and provide immediate feedback on changes. Provides inline UI hints and validation to guide users through the configuration process. Also group and manage related settings together logically, and allow for easy navigation between different sections of the configuration modal. Make sure the /config read and write to vtcode.toml and underlying configuration system correctly and consistently. Ensure fast performance and responsiveness of the modal, minimizing any delays or lag when interacting with configuration options. Provide a seamless and intuitive experience for users managing their configuration.

===

check error " I hit the tool-call safety fuse mid-verification and must stop issuing commands this turn" when VT Code is running. maybe it is because of the plan -> auto mode, should we use build mode instead?

===

add double escape to open /rewind commands

===

debug session and fix: "[!] Turn balancer: repeated low-signal navigation detected (exec::inspection:
:rg::exit ×5); scheduling an early recovery pass."

===

add code syntax highlighting for code diff previews, use existing code syntax highlighting rules and engine. '/Users/vinhnguyenxuan/Documents/vtcode-resources/idea/Screenshot 2026-09-15 at 17.15.28.png'

===

revise the diff header, it seems wrong: '/Users/vinhnguyenxuan/Documents/vtcode-resources/bugs/Screenshot 2026-09-15 at 17.16.54.png'

===

check the inline diff, it seems the gutter style colors are missing? '/Users/vinhnguyenxuan/Documents/vtcode-resources/bugs/Screenshot 2026-09-15 at 17.18.38.png'

===

allow user the choose diff themes, provide a preview of the selected theme. example themes: github, ayu, gruvbox, nord, solarized, dracula. Add new command with live preview and apply the selected theme.

===

double check compaction /compact if it works properly. "Compaction failed: Failed to generate compaction summary", check last session and debug and fix.

---

check and fix " Blocked action
exec_command is denied by workspace tool policy, so I could not run the
final extraction:
• Denial: "Tool 'exec_command' execution denied by policy";"

===

remove trailing ... dot from diff preview find another way '/Users/vinhnguyenxuan/Documents/vtcode-resources/bugs/Screenshot 2026-09-15 at 17.56.04.png'. Maybe maximize the leading and trailing spaces, since there's not enough visual separation otherwise, the edges still usable to expand the diff preview container width. observe: '/Users/vinhnguyenxuan/Documents/vtcode-resources/bugs/Screenshot 2026-09-15 at 18.04.01.png'

===

when changing model mid-turn, find a way to run auto compaction and resume for continuation from previous model to new model seamlessly. This is important to maintain the integrity of the session and avoid data and context loss.
