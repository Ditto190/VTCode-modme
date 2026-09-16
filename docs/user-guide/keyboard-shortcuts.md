# Keyboard Shortcuts

Single source of truth for VT Code terminal keyboard shortcuts. Press `?` on an empty input line while VT Code is running to open the in-app shortcut overlay (mirrors this guide).

> Platform notes:
> - `Cmd` means `Super`/`Meta` (`KeyModifiers::SUPER | META`): macOS Command, Windows Super, Linux Meta/Super. It is not `Ctrl`.
> - `Ctrl+A` remains readline move-to-start; it does not clear.
> - Terminals report `Shift+Tab` as `BackTab` (sometimes with the `SHIFT` bit); some report `Tab+SHIFT` or `Char('\t')+SHIFT`. All three cycle agents. Plain `Tab` never cycles — it enqueues.
> - Some terminals deliver `Tab` as `Char('\t')` and `Ctrl+Enter` as forked sequences; behavior is identical in both encodings.
> - `Esc Esc` must be consecutive presses within ~800ms; any other key disarms the timer.

## Composer submit and queue

| Shortcut | Idle | Active turn / busy |
| :-- | :-- | :-- |
| `Enter` | Submit draft. | Steer plain text into the active turn; slash commands keep queue routing. |
| `Ctrl+Enter` or `Tab` | Submit draft; empty draft processes newest queued message. | Queue draft (`QueueSubmit`); plain text is batchable, slash stays one-per-turn. App handles `/stop`, `/pause`, `/resume` immediately. |
| `Tab` (first) | Accept visible ghost suggestion. | Same accept-first priority. |
| `Shift+Enter` / `Alt+Enter` / `\`+`Enter` / `Ctrl+J` | Insert newline. | Same. |
| `Enter` on empty draft with active jobs | Submit `/jobs`. | Same. |

Palettes consume `Tab`/`Enter`/`Esc` first: agent/file palettes select best match on `Tab`, slash navigation autocompletes on `Tab`, history picker consumes navigation keys.

## Composer line editing (new)

| Shortcut | Single-line input | Multiline input |
| :-- | :-- | :-- |
| `Cmd+A` or `Cmd+Backspace` | Clear entire input, attachments, and compact state atomically. | Clear only the cursor's logical line; a collapsed `[Pasted Content N chars]` block overlapping the line or cursor is removed atomically. Image `[Image #N]` tokens on the line are removed with it. |
| `Cmd+Left` / `Cmd+Right` (`Shift` extends selection) | Buffer start/end. | Current logical-line start/end (logical `\n` lines, not visual wraps). |
| `Esc Esc` (focused composer, content present) | Clear entire input. | Clear current logical line. First press arms, second clears. Empty input emits cancel. |
| `Cmd+E` | Line end. | Current line end. |

`Cmd` never means `Ctrl` here. Secure-prompt (single-line secret) mirrors this: `Cmd+Backspace`/`Cmd+A` clear, `Cmd+Left/Right` jump to edges, `Esc` cancels immediately without double-press.

## Readline and word editing (unchanged)

| Shortcut | Action |
| :-- | :-- |
| `Ctrl+A` / `Ctrl+E` | Buffer start / end. |
| `Ctrl+F` / `Ctrl+B` | Char forward / back. |
| `Alt+F` / `Alt+B`, `Alt+Left/Right` | Word forward / back. |
| `Ctrl+P` / `Ctrl+N`, `Up`/`Down` | History previous / next (`Up`/`Down` move within multiline first). |
| `Ctrl+W`, `Alt+D` | Delete previous / next word. |
| `Ctrl+U` / `Ctrl+K` | Delete to line start / end. |
| `Ctrl+T` | Transpose chars (when Transcript Review is unbound; otherwise opens review). |
| `Alt+T` | Transpose words / toggle tool summaries per binding. |
| `Alt+U` / `Alt+L` / `Alt+C`, `Alt+\` | Uppercase / lowercase / capitalize word, delete whitespace around cursor. |
| `Ctrl+Z` / `Ctrl+Y` | Undo / redo. |
| `Ctrl+G` | Open external editor with draft. |
| `Home` / `End` (`Shift` selects) | Buffer start / end. |

## Agent and mode switching

| Shortcut | Action |
| :-- | :-- |
| `Shift+Tab` (`Tab+SHIFT`, `BackTab`, `Char('\t')+SHIFT`) | Cycle primary agent forward (`CyclePrimaryAgent`); `BackTab` cycles previous (`CyclePrimaryAgentPrevious`). |
| Locked (`Building`, `Recovery`, `Blocked`, busy handoff) | Switch dropped with mode-switch notice; `Tab` enqueue still works. |

## Overlays, palettes, and lists

| Context | Keys |
| :-- | :-- |
| Agent/file palette | `Up`/`Down` move, `Tab` select best match, `Enter` apply, `Esc` close. |
| Slash palette | `Tab` autocomplete, navigation via slash keys. |
| History picker (`Ctrl+R`/`Ctrl+S`) | Type to filter, `Tab`/`Esc`/`Enter` accept, `Ctrl+C` or empty `Backspace` cancel. |
| Modal list | `Tab`/`BackTab` move per modal, `Esc`/`Enter` cancel/submit per overlay. |
| Local agents drawer | `Down` opens on empty composer; `Alt+S` focuses. |
| Queued-input edit | `Alt+Up` (or `Shift+Left` in tmux) pops newest queued message into composer. |

## Transcript Review and fullscreen

| Shortcut | Action |
| :-- | :-- |
| `Ctrl+T` (default) / `Alt+O` | Open/close Transcript Review. |
| `r` | Toggle rich/raw. |
| `/`, `Enter`, `Esc`, `n`/`N` | Search start/commit/cancel, next/previous match. |
| `j`/`k`, `Up`/`Down`, `Ctrl+U`/`D`, `Ctrl+B`/`F`, `g`/`G`, `Home`/`End` | Scroll line, half-page, full-page, top/bottom. |
| `v`, `[`, `q` | Open in editor, hand to native scrollback, close. |
| `PgUp`/`PgDn`, wheel, `Ctrl+Home`/`End` | Fullscreen transcript scroll. |

## Multiline input methods

| Method | Shortcut |
| :-- | :-- |
| Quick escape | `\` + `Enter` |
| macOS default | `Option+Enter` |
| Native/configured | `Shift+Enter` (Ghostty, Kitty, WezTerm, iTerm2, Warp natively; `/terminal-setup` for VS Code, Alacritty, Zed) |
| Control sequence | `Ctrl+J` |
| Paste | Paste directly (large pastes collapse to `[Pasted Content N chars]`; one `Backspace` at block end removes the block) |

## Quick commands

| Input | Action |
| :-- | :-- |
| `#` at start | Custom prompts picker. |
| `/` at start | Slash command (`/help`, `/slash-commands` list all). |
| `!` at start | Bash mode (direct execution). |
| `@` | File picker; `@agent-<name>` subagent picker (`@agent-<plugin>:<name>` for plugins). |
| `Alt+P` | Ghost suggestion; `Tab` accepts. |

## Custom keybindings

Rebindable actions (`open_transcript_review`, `toggle_transcript_render_mode`, `toggle_tool_display_mode`, `toggle_task_panel`, `scroll_page_up/down`, `interrupt`, `exit`, history, queue edit) live under `ui.keybindings` (`KeyBindingConfig::bindings` / `UserPreferences::keybindings`, `cmd`/`super` = Command). Composer editing keys above (`Tab`, `Esc`, `Cmd+A`, arrows, readline) are intentionally hardcoded in `session/events.rs` and not rebindable. See [Configuration](../config/config.md) and [Interactive Mode](./interactive-mode.md).
