check when VT Code is in Build/Auto mode and when it needs to switch to plan mode for planning. the system already switch as per user confirmation, but the header still shows the previous mode. Check and update the header accordingly.

===

CRITICAL: it seems the background task can not be stopped or closed, and also the main runloop is stuck with loading state. Also, user can not send any new messages or commands. This needs immediate attention to prevent the application from becoming unresponsive.

screenshot: '/Users/vinhnguyenxuan/Documents/vtcode-resources/Screenshot 2026-09-24 at 11.25.18.png'

===

for multi agent or multi background queue/process handling and UI in TUI.

--> also consider implement a and more expanded background window for better visibility and management of multiple agents and background processes in the TUI when click/ on the background task indicator or relevant UI element.

reference:

1. running state '/Users/vinhnguyenxuan/Documents/vtcode-resources/Screenshot 2026-09-24 at 15.56.47.png'
2. click on the background task indicator or relevant UI element. '/Users/vinhnguyenxuan/Documents/vtcode-resources/Screenshot 2026-09-24 at 15.57.29.png'
3. finished state: '/Users/vinhnguyenxuan/Documents/vtcode-resources/Screenshot 2026-09-24 at 16.06.19.png'
4. show messages '/Users/vinhnguyenxuan/Documents/vtcode-resources/Screenshot 2026-09-24 at 16.06.23.png'
5. expanded 1 of the message '/Users/vinhnguyenxuan/Documents/vtcode-resources/Screenshot 2026-09-24 at 16.06.32.png'

===

fix plan mode rejection:

"
Plan revision rejected: invalid plan artifact:
missing sections: Summary, Test Cases and Validation,
Assumptions and Defaults; invalid implementation
steps: step 1: must include a `verify:` or `
verification:` marker; step 2: must name a concrete
file, symbol, or behavior target; step 3: must name a
concrete file, symbol, or behavior target; summary is
empty; no validation items; no assumptions or
defaults
Rejected plan revision:"

session: session-vtcode-20260924T133543Z_155288-07964
