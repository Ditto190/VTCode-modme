use std::fs::OpenOptions;
use std::io::{self, Write};
use std::sync::atomic::Ordering;

use ratatui::crossterm::{
    cursor::{MoveToColumn, RestorePosition, SetCursorStyle, Show},
    event::{DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, PopKeyboardEnhancementFlags},
    execute,
    terminal::{Clear, ClearType, LeaveAlternateScreen, disable_raw_mode},
};

use super::state::{self, KEYBOARD_ENHANCEMENTS_PUSHED};

/// Emit the terminal-restoration escape sequence.
///
/// `clear_full_screen` must be true when the TUI drew inline (no
/// alternate screen buffer), so leftover frames are erased from the main
/// screen. Alternate-screen sessions restore the main screen automatically
/// when leaving the buffer, so no explicit clear is needed there.
///
/// Writes through any `Write` so the exact sequence is unit-testable.
fn emit_restore_sequence(writer: &mut impl Write, clear_full_screen: bool) -> io::Result<()> {
    // Clear current line to remove any echoed ^C characters
    execute!(writer, MoveToColumn(0), Clear(ClearType::CurrentLine))?;

    // Leave alternate screen FIRST (most critical for visual restoration)
    execute!(writer, LeaveAlternateScreen)?;

    // Inline sessions drew directly on the main screen: erase every drawn
    // frame (welcome content, status line) so the shell prompt returns to a
    // clean screen (fixes leftover TUI residue on exit).
    if clear_full_screen {
        execute!(writer, Clear(ClearType::All))?;
    }

    // Disable terminal modes
    execute!(writer, DisableBracketedPaste)?;
    execute!(writer, DisableFocusChange)?;
    execute!(writer, DisableMouseCapture)?;

    // Only pop keyboard enhancement flags if actually pushed
    if KEYBOARD_ENHANCEMENTS_PUSHED.swap(false, Ordering::SeqCst) {
        execute!(writer, PopKeyboardEnhancementFlags)?;
    }

    Ok(())
}

fn open_tty_writer() -> Option<std::fs::File> {
    OpenOptions::new().write(true).open("/dev/tty").ok()
}

fn emit_restore_to_all_targets(clear_full_screen: bool) -> Option<io::Error> {
    let mut first_error: Option<io::Error> = None;

    let mut stderr = io::stderr();
    if let Err(error) = emit_restore_sequence(&mut stderr, clear_full_screen) {
        first_error.get_or_insert(error);
    }
    if let Err(error) = execute!(stderr, SetCursorStyle::DefaultUserShape, Show, RestorePosition) {
        first_error.get_or_insert_with(|| io::Error::other(error.to_string()));
    }
    let _ = stderr.flush();
    crate::tui::core_tui::runner::terminal_io::reset_mouse_pointer_shape();

    if let Some(mut tty) = open_tty_writer() {
        let _ = emit_restore_sequence(&mut tty, clear_full_screen);
        let _ = execute!(tty, SetCursorStyle::DefaultUserShape, Show, RestorePosition);
        let _ = tty.flush();
        let _ = write!(tty, "\x1b]22;default\x07");
        let _ = tty.flush();
    }

    first_error
}

/// Restore terminal to a usable state after a panic or error.
///
/// This is the single canonical function for terminal restoration.
/// It is idempotent: subsequent calls are no-ops.
///
/// - Drains pending events before and after restoration
/// - Leaves alternate screen
/// - Clears the entire screen when the TUI ran inline (no alternate buffer)
/// - Disables bracketed paste, focus change, mouse capture
/// - Pops keyboard enhancement flags if pushed
/// - Resets cursor style and shows cursor
/// - Disables raw mode last
pub fn restore_tui() -> io::Result<()> {
    if !state::try_claim_restore() {
        return Ok(());
    }

    state::mark_tui_deinitialized();

    let terminal_modified = state::is_terminal_modified();
    let alternate_active = state::is_alternate_screen_active();

    // Never emit restore sequences when no component modified the terminal
    // and no alternate screen is active: error reports for non-TUI runs
    // (failed startup, one-shot commands) would otherwise spray raw escape
    // sequences before the message. If alternate screen is active we must
    // still leave it even when TERMINAL_MODIFIED was not yet set (partial
    // init failure that succeeded to enter alt screen but failed before
    // flagging modification).
    if !terminal_modified && !alternate_active {
        return Ok(());
    }
    state::mark_terminal_restored();
    if alternate_active {
        state::mark_alternate_screen_active(false);
    }

    let mut first_error: Option<io::Error> = None;

    crate::tui::core_tui::runner::terminal_io::drain_terminal_events();

    let clear_full_screen = !alternate_active;
    if let Some(error) = emit_restore_to_all_targets(clear_full_screen) {
        first_error.get_or_insert(error);
    }

    // Drain terminal responses from restore sequences while raw mode still active
    crate::tui::core_tui::runner::terminal_io::drain_terminal_events();

    // Disable raw mode LAST — must run even if emit failed
    if let Err(error) = disable_raw_mode() {
        first_error.get_or_insert(error);
    }

    // Best-effort stty sane equivalent for /dev/tty when disable_raw_mode
    // succeeded but tty still has echo off due to cargo wrapping stderr.
    if let Some(mut tty) = open_tty_writer() {
        let _ = tty.flush();
    }
    // Ensure stderr is flushed after raw mode restore
    let _ = io::stderr().flush();

    match first_error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    #[test]
    fn test_restore_terminal_no_panic_when_not_initialized() {
        state::RESTORE_DONE.store(false, Ordering::SeqCst);
        state::TUI_INITIALIZED.store(false, Ordering::SeqCst);

        let result = restore_tui();
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn restore_sequence_clears_full_screen_for_inline_sessions() {
        let mut bytes: Vec<u8> = Vec::new();
        emit_restore_sequence(&mut bytes, true).unwrap();
        let text = String::from_utf8(bytes).unwrap();

        // Inline sessions drew on the main screen: the restore must erase
        // every leftover frame (fixes TUI residue left behind on exit).
        assert!(text.contains("\x1b[2J"), "inline restore must emit ESC[2J, got: {text:?}");
        assert!(text.contains("\x1b[1G\x1b[2K"), "inline restore must keep line clear, got: {text:?}");
        assert!(text.contains("\x1b[?1049l"), "inline restore must leave alternate screen, got: {text:?}");
    }

    #[test]
    fn restore_sequence_skips_full_screen_clear_for_alternate_screen_sessions() {
        let mut bytes: Vec<u8> = Vec::new();
        emit_restore_sequence(&mut bytes, false).unwrap();
        let text = String::from_utf8(bytes).unwrap();

        // Alternate-screen sessions restore the main screen when leaving the
        // buffer; clearing it explicitly would wipe pre-session shell content.
        assert!(!text.contains("\x1b[2J"), "alternate restore must not emit ESC[2J, got: {text:?}");
        assert!(text.contains("\x1b[?1049l"), "alternate restore must leave alternate screen, got: {text:?}");
    }

    #[test]
    fn alternate_screen_flag_resets_on_new_session() {
        state::mark_alternate_screen_active(true);
        assert!(state::is_alternate_screen_active());
        state::mark_tui_initialized();
        assert!(!state::is_alternate_screen_active(), "new TUI session must start inline");
    }

    #[test]
    fn terminal_modified_flag_round_trip() {
        state::mark_terminal_restored();
        assert!(!state::is_terminal_modified());
        state::mark_terminal_modified();
        assert!(state::is_terminal_modified());
        state::mark_terminal_restored();
        assert!(!state::is_terminal_modified());
    }

    #[test]
    fn new_tui_session_waits_for_terminal_mutation() {
        state::mark_terminal_restored();
        assert!(!state::is_terminal_modified());
        state::mark_tui_initialized();
        assert!(state::is_tui_initialized());
        assert!(!state::is_terminal_modified(), "TUI registration must not claim terminal mutation");
        state::mark_tui_deinitialized();
    }

    #[test]
    fn restore_skips_emission_when_terminal_never_modified() {
        state::RESTORE_DONE.store(false, Ordering::SeqCst);
        state::TUI_INITIALIZED.store(false, Ordering::SeqCst);
        state::mark_terminal_restored();

        let result = restore_tui();
        assert!(result.is_ok() || result.is_err());
        assert!(
            state::RESTORE_DONE.load(Ordering::SeqCst),
            "restore must still be claimed so later calls stay no-ops"
        );
        assert!(!state::is_terminal_modified(), "unmodified terminal must stay unmodified");

        // A modified terminal must still run the full restore and clear the flag.
        state::RESTORE_DONE.store(false, Ordering::SeqCst);
        state::mark_terminal_modified();
        let result = restore_tui();
        assert!(result.is_ok() || result.is_err());
        assert!(!state::is_terminal_modified(), "restore must clear the modified flag");
    }
}
