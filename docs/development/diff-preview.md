# Diff Preview Architecture

VT Code computes text changes in the publishable `vtcode-diff` crate. The crate
has no dependency on other VT Code crates: it owns bounded `similar`-based diff
computation, the semantic document model, intraline byte ranges, Unicode-width
wrapping, unified/side-by-side layout, and bounded excerpts.

Application crates own policy and presentation:

- `vtcode-ui` caches the document when an overlay opens and only relayouts or
  scrolls it during resize and navigation.
- theme and terminal capability selection remain in the VT Code design system;
  diff styling uses a subtle full-row add/delete tint and a stronger
  background for changed intraline spans. Context rows remain un-tinted,
  while hunk and file metadata use bold syntax-highlighted foregrounds with no
  background. ANSI16 terminals and explicit no-color output fall back to
  foreground-only styling.
- syntax highlighting is injectable application work, so the reusable crate
  does not depend on `syntect` or VT Code themes.
- approval state, modal priority, `Enter`/`Esc` behavior, and runtime events are
  unchanged and remain outside the diff crate.

`DiffOptions` defaults to practical Myers with separate line and intraline
timeouts. Patience and Histogram are public library choices but are not config
values. For large output, preserve readable semantic rows and disable costly
refinement before suppressing content. `LayoutOptions::max_rows` produces a
head/tail excerpt with an explicit omission row.

The user-facing unified layout is still configured as `"inline"` for backward
compatibility. Width decisions use the measured content width after message
indentation and transcript framing: side-by-side falls back to unified below
the shared 60-column threshold, and either unified presentation hides its
visible `+`/`-`, line-number, and `│` gutter when keeping it would leave less
than 20 columns for source text. The gutter and side-by-side panes return
automatically when a resize restores enough width. Tests should cover
asymmetric insert/delete cases, UTF-8 boundaries, display-width wrapping,
deadlines, pairing, exact omission accounting, and both sides of these width
boundaries.

The presentation architecture was informed by the open-source Codex terminal
diff design. VT Code's implementation and state model remain independent.
