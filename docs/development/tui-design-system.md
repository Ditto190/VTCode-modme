# TUI design system

Canonical list UI lives in **`vtcode_ui::design`**. Wire types stay in
`vtcode_commons::ui_protocol` (protocol layer between the agent and the TUI).

## Modules

| Module | Role |
| --- | --- |
| `design::list` | Row factories: `group_header`, `group_divider`, `hint`, `setting`, `action`, `choice`, `current_choice` |
| `design::keys` | Keyboard/interaction copy (`list_hint()`, `choice_hint()`) |
| `design::constants` | Spacing tokens and `VALUE_COL` (value-column width) |
| `design::layout` / `panel` / `color` | Layout modes, panel chrome, color bridging |

## Row contract

- **Group header** — bold title, blank spacing above and below. Non-selectable.
- **Setting** — `title` + accent `value` + dimmed description. Tone via `badge_tone`.
- **Action** — imperative row with explicit tone (Neutral/Accent/Success/Warning/Danger).
- **Choice / current_choice** — list options; `current_choice` marks `Current`.
- **Hint** — dimmed note; never a header.
- **group_divider** — full-width rule between sections.

Do **not** assemble `InlineListItem { .. }` literals in command/UI code; call the factories and tweak with `with_search_value` / `with_badge` when needed.

## Interaction contract

- Filter-aware lists: printables type into the filter; Esc clears, then backs/closes.
- `↑↓ select · Enter apply · ←→ value · type to filter · Esc back` (`design::keys::list_hint`).
- Standalone pickers: `design::keys::choice_hint` (`↑↓/jk move · … · Esc clear/cancel`).
- Nested views: Esc = Back; root Esc = close. Empty filter result blocks Enter.

## Accessibility

- Status and badges always include text (not color alone).
- Headers and hints are non-selectable so screen readers and keyboard users are not trapped on chrome rows.
- Value column uses `constants::VALUE_COL` so scanning stays consistent.

## When adding a modal

1. Build rows with `design::list`.
2. Use `design::keys` for footer/hint copy.
3. Prefer `setting` for anything with a live value; `action` for commands.
4. Group long lists with `group_header` / `group_divider` — the renderer supplies spacing.
