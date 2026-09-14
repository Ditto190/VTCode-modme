Improve apply/edit patch UI

1. Change the apply/edit patch UI to be more intuitive.
2. style and syntax highlight the file and diff color
3. ~~on preview: change the word "too_many_changes" to be more user-friendly~~ reason code kept stable; renderers show `diff_preview_user_message`
4. Improve and streamline the overall UI of file ops UI/UX in the TUI
5. Follow design system

---

Fix diff preview TUI.

1. ~~remove the background color in the diff header.~~ only use diff foreground color only (red/green ansi)
2. ~~check and fix the background color in the diff body, it still have missing holes and blank gap~~ solid foreground, no gaps
3. ~~also make the text's foreground color aligned~~ aligned with marker

--

check VT Code mode planning: "Planning workflow remains active: no approval-ready plan was produced. Keep planning and describe what to revise.
• The proposed plan was rejected and discarded; no continuation turn was scheduled. Revise the plan or restate the
request to continue (invalid plan artifact: invalid implementation steps: step 4: verification item 1 must be a
concrete command or check)."

Plan is not ready for approval: invalid plan artifact: invalid implementation steps: step 4: verification item 1 must
be a concrete command or check
Rejected plan draft:

--> VT Code check plan mode plan draft prososal state machines. it seems the plan is being rejected due to invalid implementation steps. and VT Code seems unable to recover until the proper plan is provided and the invalid steps are corrected. It should be revised to include concrete commands or checks for verification items in step 4. and provide a proposal plan for user to follow.

--> check session: session-vtcode-20260914T031505Z_075199-09813
