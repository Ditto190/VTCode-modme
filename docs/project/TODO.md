# TODO

## Session event-log durability (#744)

- [x] Make event-log compaction and manifest/index writes atomic and fsynced.
- [x] Recover from malformed metadata and reject stale turn-index offsets.
- [x] Enforce private session directories (`0700`) and artifacts (`0600`).
- [x] Add compaction, recovery, reopen, and permission regression coverage.

## UTF-8-safe text boundaries (#745)

- [x] Replace raw byte-offset truncation and slicing in the reported runtime paths with boundary-safe helpers.
- [x] Add regression coverage for non-ASCII PTY input, Vim vertical movement, notifications, excerpts, and related truncation paths.
- [x] Run focused quality checks, then reply to and close the issue.

## list_files path containment (#746)

- [x] Validate every listing mode through the symlink-aware workspace path policy.
- [x] Add traversal and absolute-path regression coverage for all listing modes.

## Memory fixes

- [ ] Investigate and fix the memory feature not saving user context.
    - **Observed:** After asking VT Code to remember that the user is Vinh Nguyen / `vinhnx`, the assistant reported: “Couldn't save memory because the LLM planner still needs more information.”
    - **Expected:** A sufficiently specific, user-approved fact should be persisted to the session-independent memory store and be available in later turns.
    - **Reproduction context:** The request followed a conversation in which `vinhnx` was identified from local repository metadata and public profiles. The save attempt failed before any confirmation that a memory file or durable store entry was created.
    - **Acceptance criteria:** - Saving a clear user preference or identity alias does not require unrelated planner information. - The user receives an actionable error when persistence fails, including what additional information is required. - A successful save is verified by reading the memory through the supported memory path in a subsequent turn. - Add regression coverage for the planner/memory-save path and the failure message above.
      log: local `.vtcode/checkpoints/` artifacts (`turn_995.json` through `turn_991.json`)

session: local `.vtcode/sessions/` artifact for the reproduction

---

CRITICAL: check vtcode post-amble summarized session is sometimes gone/missing. it was working before. context: when user control+c or quit the program, there is the summarization turn/context shown in the CLI. Currently it showing a blank space. This is a regression from the previous behavior. The summarization turn/context should be shown in the CLI after the user quits the program.

===

implement intelligent container width for display table/blocks

if width is enough use the wide table-layout regression screenshot
if not use the narrow heading-block regression screenshot

===

implement /secret filter

===

on /config, revise the UI and when go back from section -> keep previous selected entry. also revamp the /config UI to be more user friendly and intuitive. implement live reload of config changes without needing to restart the program. implement a /config reset command to reset all config to default values. Both in TUI and CLI.

===

add glm-5.3 to huggingface

curl https://router.huggingface.co/v1/chat/completions \
 -H "Authorization: Bearer $HF_TOKEN" \
 -H 'Content-Type: application/json' \
 -d '{
"messages": [
{
"role": "user",
"content": "What is the capital of France?"
}
],
"model": "zai-org/GLM-5.3:together",
"stream": false
}'

---

https://ollama.com/library/glm-5.3-flash

===

https://ollama.com/library/glm-5.3
