support this https://github.com/openai/codex/issues/42294

==

check and cleanup TUI's user facing diagnostic messages and verbose error/info output. Keep it clean, short and concise. As most of the time users only need the essential information, avoid overwhelming them with too much detail. The information is targeted for the coding agent's consumption only.

===

check when VT Code is in Build/Auto mode and when it needs to switch to plan mode for planning. the system already switch as per user confirmation, but the header still shows the previous mode. Check and update the header accordingly.
