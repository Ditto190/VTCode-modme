# Tool Summary Display

VT Code has two independent tool-display settings:

- `ui.tool_output_mode` controls how command and tool result bodies are truncated or spooled.
- `ui.tool_display_mode` controls the transition summaries shown before those bodies. It defaults to `compact`; contiguous successful command/PTY calls share a compact activity row while non-command tools retain their compact tree formatting.

```toml
[ui]
tool_display_mode = "compact"
```

Compact mode keeps one concise activity row for each contiguous run of
successful command/PTY calls. A single command can show its command and hidden
line count, for example `• Ran cargo check · … +12 lines`; consecutive calls
collapse to `• Ran 2 commands`. The review suffix includes the configured
shortcut and a styled `click to expand or collapse` affordance. In expanded
mode, running PTY blocks show a bounded live tail; after completion, successful
bodies collapse to the activity row.
Failures, non-zero exits, cancellations, warnings, stderr diagnostics,
meaningful diffs, and useful artifacts retain bounded inline context. Complete
output is available in the session-local Transcript Review opened with the
configured review shortcut, including outside fullscreen, where it is ordered with user messages, assistant
responses, reasoning, and other status entries. Rich review rendering is the
default; `r` switches to ANSI-free raw text. Plan updates, non-command tools,
and explicit result bodies end a command group. The review hint follows the
configured primary review binding and is omitted when that action is unbound.
Successful file mutations also end a command group and remain visible as a
glanceable `• Edited path (+N -M)` (or created/deleted) row followed by the
numbered diff preview; the complete tool result remains available to the
agent and Transcript Review.

In compact mode, PTY commands keep their complete capture and grouped completion row without emitting a transient live PTY block. Progress remains available through the active status/spinner, while warnings, failures, diffs, stderr, and meaningful artifacts stay inline. Expanded mode preserves the bounded live tail.

The runtime mode can be changed for the current session with `Alt+T`. This action is rebindable through the existing keybinding configuration. Use `/config` to cycle `ui.tool_display_mode` and persist the choice to `vtcode.toml`.

Explicit `expanded` mode preserves the existing per-call summary and live-output layout.

## Model-visible tool output budget

Tool-result previews copied into provider-facing history share a 32 KiB
aggregate budget per turn. The existing per-result spool limit still applies;
when the aggregate budget is exhausted, later results expose only bounded
metadata such as the tool name, spool path, byte count, completion state, and a
short note. Complete output remains in the internal spool and current-session
tool-output viewer, so this budget affects recovery diagnostics rather than
Transcript Review.

Static, bounded reads of `.vtcode/context/tool_outputs/` using `cat`, `sed`,
`tail`, or bounded `rg` pipelines are marked `no_spool` before generic output
processing. Dynamic paths, redirects, writes, in-place edits, and malformed
commands remain fail-closed; they cannot opt out of normal spooling.
