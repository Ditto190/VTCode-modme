Improve apply/edit patch UI

1. Change the apply/edit patch UI to be more intuitive.
2. style and syntax highlight the file and diff color on the edit patch
3. reason code kept stable; renderers show `diff_preview_user_message`
4. Improve and streamline the overall UI of file ops UI/UX in the TUI
5. Follow design system

---

https://github.com/mitsuhiko/similar

also check https://deepwiki.com/search/how-codex-implement-diff-previ_17050125-2468-4dce-94da-0a0ea6c0af6b?mode=fast and refer to the implementation details for rewrite the diff preview UI. and also consider open source it.

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

check VT Code notification from terminal/ghostty etc doesn't showing in the notification center properly. Bug: notifications are not appearing as expected, making it difficult for the user to stay informed about important events and updates. Currently, only the terminal get pinged, and the notification center remains empty and I am missing critical notifications. (Note: currently tested on macOS and ghostty, other platforms may have similar issues.)

===
