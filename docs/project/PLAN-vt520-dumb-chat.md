# PLAN: VT520 Dumb-Terminal Chat Mode

Status: plan only, not implemented.
Scope: emulator-only testing, chat + tools (full agent loop with approvals).
No code changes in this plan.

## 1. Goal

Run `vtcode` as a plain line-buffered chatbot on a DEC VT520 (or
`TERM=vt520` emulator): no fullscreen TUI, no raw mode, no redraws.
Support multi-turn chat with tool use (`exec`, `read`, `write`) and
numbered approval prompts that work at 80 columns over a slow serial-style
link. This mirrors the user's current Python-script + API-key bot, but on
the `vtcode` agent loop.

Non-goals: porting the full `ratatui` TUI, Sixel/ReGIS graphics, mouse,
bracketed paste, Kitty keyboard, clipboard OSC52, truecolor themes.

## 2. Background and constraints

VT520 facts (mono text-only; VT525 is the color variant):

- 80/132 columns, VT100/VT52/Wyse/ANSI emulation, DEC Special Graphics,
  soft chars via sixel download. Text-only, unlike VT340 (no ReGIS/vector).
- Up to 115.2 kbit/s serial, up to 4 sessions (TD/SMP). No UTF-8
  box-drawing, max ~16 colors, no alternate screen `1049`, no `2004/1004/
  1006/>1u/2026/OSC` sequences.

`vtcode` today (`Cargo.toml:37,74`: `crossterm 0.29`, `ratatui =0.30.2`):

- Interactive `chat`/`continue`/bare always enters a raw-mode event loop on
  `stderr` (`crates/codegen/vtcode-ui/src/tui/session_options.rs:87-92`,
  `tui/runner/mod.rs:275-276`) and always tries `2004/1004`/Kitty/`OSC22`
  (`tui/runner/terminal_modes.rs:136-168`) plus a `/dev/tty` OSC palette
  probe (`crates/codegen/vtcode-core/src/utils/terminal_color_probe.rs:83-95`).
- `TERM=dumb` only disables color/unicode
  (`ansi_capabilities.rs:27`, `terminal_capabilities.rs:83-88`); it still
  tries TUI when `stdin` is a TTY.
- Agent loop itself is TUI-agnostic. Headless one-shots already bypass
  `run_tui` (`src/main.rs:229-233`, `src/cli/action_resolution.rs:28-33`,
  `src/startup/mod.rs:100-160`): `ask`, `exec`, `-p/--print`, `review`,
  `--full-auto`. CLI defined in
  `crates/codegen/vtcode-core/src/cli/args/mod.rs:177-205,321-377`.

Conclusion: full TUI port is not viable; a dumb `chat` loop reusing the
headless runtime is.

## 3. What already works (VT520-safe today)

```sh
TERM=dumb NO_COLOR=1 vtcode ask "hello" --no-color
TERM=dumb NO_COLOR=1 vtcode -p "explain foo" < input.txt
echo "ctx" | vtcode exec "review this" --no-color
vtcode review --no-color
```

Shell-loop chatbot workaround (no session continuity, batched output):

```sh
while IFS= read -r q; do vtcode exec "$q" --no-color 2>&1; done
```

Limits: `ask`/`exec` batch output (`commands/ask.rs:75-90`,
`single_response.rs:7-55`); human event lines still use `style()` unless
`NO_COLOR` (`exec/event_output.rs:241-390`).

## 4. Proposed design: `chat --dumb`

New `ResolvedCliAction::DumbChat` (`src/cli/action_resolution.rs`,
`src/cli/dispatch/commands.rs`, `StartupPolicy::for_args` in
`src/startup/mod.rs:100-160`): reuse `run_single_agent_loop`
(`src/agent/agents.rs:125-167`) with a dumb printer instead of
`spawn_session_with_options` (`session_setup/ui.rs:244-274`).

Loop sketch (provider-agnostic; model is `codex/runtime.rs:836-848`):

```text
while print!("> "); flush; read_line() -> Some(q) except exit/quit/EOF:
    execute_turn(q) -> writeln!(stdout, answer); writeln!(stdout, "[files: ...]")
```

- Streaming: wire `StreamProgressEvent::OutputDelta`
  (`runloop/unified/ui_interaction.rs:661,720`,
  `ui_interaction_stream.rs:210-294`) to `print!` + flush, line-buffered
  for slow links. Reuse `ExecEventProcessor`
  (`src/cli/exec/event_output.rs:56-94`) with a plain `human_event_line`.
- Caps early-out for `TERM=dumb|vt52*|vt100|vt520`: skip palette probe,
  force `Inline` surface + color never + ASCII borders + 80-col wrap
  (`get_terminal_width` already falls back to 80 in
  `core/src/utils/tty.rs:193-195`). Never `open("/dev/tty")`,
  `enable_raw_mode`, `EnterAlternateScreen`, Kitty/mouse/focus/OSC52.
  Touch points: `terminal_detection.rs:44-98`,
  `terminal_capabilities.rs:57-110`, `ansi_capabilities.rs:236-266`.
- Approvals (required for chat+tools): replace `dialoguer Select/Confirm`
  (`codex_app_server/runtime.rs:778-783`, `core/src/ui/user_confirmation.rs:8`,
  `startup/workspace_trust.rs:231-267`) with `1) approve 2) deny` +
  `read_line`, fail-closed on EOF/pipe. Trust via
  `VTCODE_TRUST_WORKSPACE=full-auto` only, matching `exec-mode.md` order.
- I/O: lossy read instead of `read_to_string`
  (`main_helpers/prompt_input.rs:38-39`, `exec/prep.rs:291-293`),
  `strip_ansi`, ASCII table fallback. Force `pipe` backend
  (`vtcode-bash-runner/src/pipe.rs:92-167`) + `NO_COLOR=1` child env.

## 5. Verification (emulator-only)

- `TERM=vt520 NO_COLOR=1`, `stty cols 80`, `xterm -ti vt520`, `vttest`.
- `./scripts/check-dev.sh --test`; `cargo nextest run -p vtcode-core -E 'binary(/pty_tests/)'`.
- Manual: multi-turn chat, tool call with numbered approval, 80-col wrap,
  `NO_COLOR=1` child output, clean EOF/`/exit` handling.
- Per root `AGENTS.md`: user-facing behavior needs a `docs/development/`
  guide + quick-reference row; agent tool-surface changes need
  `prompts/guidelines.rs` + `vtcode-utility-tool-specs` schema updates;
  runtime contract changes need `ThreadEvent` updates. Keep local
  `AGENTS.md` links resolving.

## 6. References

- Loop substrate: [PLAN-loop-engineering.md](PLAN-loop-engineering.md)
- Headless contract: [agent-loop-contract.md](../guides/agent-loop-contract.md),
  [exec-mode.md](../user-guide/exec-mode.md)
- PTY vs pipe: [PTY_PIPE_INFRASTRUCTURE.md](../pty/PTY_PIPE_INFRASTRUCTURE.md),
  [pty-integration-testing.md](../guides/pty-integration-testing.md)
- TODO record: [TODO.md](TODO.md) (`VT520 dumb-chat mode` entry)
