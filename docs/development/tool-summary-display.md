# Tool Summary Display

VT Code has two independent tool-display settings:

- `ui.tool_output_mode` controls how command and tool result bodies are truncated or spooled.
- `ui.tool_display_mode` controls the transition summaries shown before those bodies. It defaults to `compact`; adjacent command calls collapse into one counted summary with a `Ctrl+T` transcript hint, while non-command tools retain their compact tree formatting.

```toml
[ui]
tool_display_mode = "compact"
```

Compact command groups suppress repeated command bodies, `$` detail rows, empty-output markers, and duplicate completion noise in the live view. The complete PTY capture remains available in transcript review. Plan updates, non-command tools, failures, warnings, cancellations, diffs, and result bodies retain their existing boundaries and compact tree formatting.

The runtime mode can be changed for the current session with `Alt+T`. This action is rebindable through the existing keybinding configuration. Use `/config` to cycle `ui.tool_display_mode` and persist the choice to `vtcode.toml`.

Explicit `expanded` mode preserves the existing per-call summary layout.
