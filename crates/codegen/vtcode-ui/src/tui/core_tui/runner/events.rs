use std::sync::atomic::AtomicU64;
use std::time::{Duration, Instant};

use futures::{FutureExt, StreamExt};
use ratatui::crossterm::event::{Event as CrosstermEvent, KeyCode, KeyEventKind, MouseEventKind};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, error::TryRecvError};
use tokio_util::sync::CancellationToken;

use super::TuiSessionDriver;
use crate::tui::config::constants::ui;

#[derive(Debug, Clone)]
pub(crate) enum TerminalEvent {
    Tick,
    Crossterm(CrosstermEvent),
}

#[derive(Clone)]
pub(super) struct EventChannels {
    pub(super) tx: UnboundedSender<TerminalEvent>,
    pub(super) rx_paused: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Tracks last input time for adaptive tick rate (milliseconds since session start)
    pub(super) last_input_elapsed_ms: std::sync::Arc<AtomicU64>,
    /// Session start time for calculating elapsed time
    pub(super) session_start: Instant,
}

impl EventChannels {
    fn new(tx: UnboundedSender<TerminalEvent>) -> Self {
        Self {
            tx,
            rx_paused: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            last_input_elapsed_ms: std::sync::Arc::new(AtomicU64::new(0)),
            session_start: Instant::now(),
        }
    }

    pub(super) fn pause(&self) {
        self.rx_paused.store(true, std::sync::atomic::Ordering::Release);
    }

    pub(super) fn resume(&self) {
        self.rx_paused.store(false, std::sync::atomic::Ordering::Release);
    }

    /// Record that user input was received (updates last input timestamp)
    /// Uses Instant-based tracking for efficiency (no syscalls)
    pub(super) fn record_input(&self) {
        let elapsed_ms = self.session_start.elapsed().as_millis() as u64;
        self.last_input_elapsed_ms
            .store(elapsed_ms, std::sync::atomic::Ordering::Release);
    }
}

pub(super) struct EventListener {
    receiver: UnboundedReceiver<TerminalEvent>,
}

impl EventListener {
    pub(super) fn new() -> (Self, EventChannels) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let channels = EventChannels::new(tx);
        (Self { receiver: rx }, channels)
    }

    pub(super) async fn recv(&mut self) -> Option<TerminalEvent> {
        self.receiver.recv().await
    }

    pub(super) fn try_recv(&mut self) -> Result<TerminalEvent, TryRecvError> {
        self.receiver.try_recv()
    }

    /// Clear all queued events from the input channel
    pub(super) fn clear_queue(&mut self) {
        while self.receiver.try_recv().is_ok() {
            // Keep draining until empty
        }
    }
}

/// Represents accumulated scroll events for coalescing
pub(super) struct ScrollAccumulator {
    line_delta: i32,
    page_delta: i32,
    wheel_step: i32,
}

impl ScrollAccumulator {
    pub(super) fn new(scroll_speed: u8) -> Self {
        Self {
            line_delta: 0,
            page_delta: 0,
            wheel_step: i32::from(scroll_speed.max(1)),
        }
    }

    /// Returns false when the event must be dispatched after flushing pending scroll.
    /// Handles mouse scroll wheel events and PageUp/PageDown keyboard events.
    pub(super) fn try_accumulate(&mut self, event: &CrosstermEvent) -> bool {
        let (line_delta, page_delta): (i32, i32) = match event {
            CrosstermEvent::Mouse(mouse) => match mouse.kind {
                MouseEventKind::ScrollDown => (self.wheel_step, 0),
                MouseEventKind::ScrollUp => (-self.wheel_step, 0),
                _ => return false,
            },
            CrosstermEvent::Key(key) if matches!(key.kind, KeyEventKind::Press) => match key.code {
                KeyCode::PageUp => (0, -1),
                KeyCode::PageDown => (0, 1),
                _ => return false,
            },
            _ => return false,
        };
        if self.has_scroll()
            && (self.line_delta.signum() != line_delta.signum() || self.page_delta.signum() != page_delta.signum())
        {
            return false;
        }
        let Some(next_line_delta) = self.line_delta.checked_add(line_delta) else {
            return false;
        };
        let Some(next_page_delta) = self.page_delta.checked_add(page_delta) else {
            return false;
        };
        self.line_delta = next_line_delta;
        self.page_delta = next_page_delta;
        true
    }

    /// Check if there are any accumulated scroll events
    pub(super) fn has_scroll(&self) -> bool {
        self.line_delta != 0 || self.page_delta != 0
    }

    /// Apply accumulated scroll to the session using the coalesced scroll method
    pub(super) fn apply<S: TuiSessionDriver>(&self, session: &mut S) {
        if self.has_scroll() {
            session.apply_coalesced_scroll(self.line_delta, self.page_delta);
            session.mark_dirty();
        }
    }
}

// Spawn the async event loop with proper cancellation token support
// Uses crossterm::event::EventStream for async-native event handling
// Implements adaptive tick rate: 60Hz when active, 10Hz when idle
pub(super) async fn spawn_event_loop(
    event_tx: UnboundedSender<TerminalEvent>,
    cancellation_token: CancellationToken,
    rx_paused: std::sync::Arc<std::sync::atomic::AtomicBool>,
    last_input_elapsed_ms: std::sync::Arc<AtomicU64>,
    session_start: Instant,
) {
    let mut reader = crossterm::event::EventStream::new();
    let active_tick_duration = Duration::from_secs_f64(1.0 / ui::TUI_ACTIVE_TICK_RATE_HZ);
    let idle_tick_duration = Duration::from_secs_f64(1.0 / ui::TUI_IDLE_TICK_RATE_HZ);
    let active_timeout_ms = ui::TUI_ACTIVE_TIMEOUT_MS;

    let mut last_tick = Instant::now();

    loop {
        // Determine current tick rate based on recent activity (using Instant, no syscalls)
        let last_input = last_input_elapsed_ms.load(std::sync::atomic::Ordering::Acquire);
        let is_active = if last_input == 0 {
            false
        } else {
            let current_elapsed = session_start.elapsed().as_millis() as u64;
            current_elapsed.saturating_sub(last_input) < active_timeout_ms
        };

        let tick_duration = if is_active {
            active_tick_duration
        } else {
            idle_tick_duration
        };

        // Calculate remaining time until next tick
        let elapsed = last_tick.elapsed();
        let sleep_duration = tick_duration.saturating_sub(elapsed);

        let crossterm_event = reader.next().fuse();

        tokio::select! {
            _ = cancellation_token.cancelled() => {
                break;
            }
            maybe_event = crossterm_event => {
                match maybe_event {
                    // Only send if not paused. When paused (e.g., during external editor launch),
                    // skip sending to prevent processing input while the editor is active.
                    Some(Ok(evt)) if !rx_paused.load(std::sync::atomic::Ordering::Acquire) => {
                        let _ = event_tx.send(TerminalEvent::Crossterm(evt));
                    }
                    Some(Ok(_)) => {}
                    Some(Err(error)) => {
                        tracing::error!(%error, "terminal event stream error");
                    }
                    None => break,
                }
            }
            _ = tokio::time::sleep(sleep_duration) => {
                let _ = event_tx.send(TerminalEvent::Tick);
                last_tick = Instant::now();
            }
        }

        if event_tx.is_closed() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyModifiers, MouseEvent};

    fn wheel(kind: MouseEventKind) -> CrosstermEvent {
        CrosstermEvent::Mouse(MouseEvent {
            kind,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        })
    }

    #[test]
    fn scroll_accumulator_keeps_reversals_separate_at_both_bounds() {
        for (initial_offset, first_kind, second_kind, expected_offset) in [
            (0, MouseEventKind::ScrollDown, MouseEventKind::ScrollUp, 3),
            (10, MouseEventKind::ScrollUp, MouseEventKind::ScrollDown, 7),
        ] {
            let mut pending = ScrollAccumulator::new(3);
            assert!(pending.try_accumulate(&wheel(first_kind)));
            assert!(!pending.try_accumulate(&wheel(second_kind)));
            let offset = (initial_offset - pending.line_delta).clamp(0, 10);
            let mut next = ScrollAccumulator::new(3);
            assert!(next.try_accumulate(&wheel(second_kind)));
            assert_eq!((offset - next.line_delta).clamp(0, 10), expected_offset);
        }
    }

    #[test]
    fn scroll_accumulator_preserves_wheel_page_order() {
        let mut pending = ScrollAccumulator::new(3);
        assert!(pending.try_accumulate(&wheel(MouseEventKind::ScrollDown)));
        let page_up = CrosstermEvent::Key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE));
        assert!(!pending.try_accumulate(&page_up));
        assert_eq!((pending.line_delta, pending.page_delta), (3, 0));
    }

    #[test]
    fn scroll_accumulator_merges_same_direction_and_rejects_overflow() {
        let mut pending = ScrollAccumulator::new(0);
        let down = wheel(MouseEventKind::ScrollDown);
        assert!(pending.try_accumulate(&down));
        assert!(pending.try_accumulate(&down));
        assert_eq!(pending.line_delta, 2);
        pending.line_delta = i32::MAX;
        assert!(!pending.try_accumulate(&down));
        assert_eq!(pending.line_delta, i32::MAX);
    }

    #[test]
    fn scroll_accumulator_preserves_page_reversal_and_release_dispatch() {
        let mut pending = ScrollAccumulator::new(3);
        let page_up = CrosstermEvent::Key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE));
        let page_down = CrosstermEvent::Key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE));
        assert!(pending.try_accumulate(&page_up));
        assert!(!pending.try_accumulate(&page_down));
        assert!(!pending.try_accumulate(&CrosstermEvent::Key(KeyEvent::new_with_kind(
            KeyCode::PageUp,
            KeyModifiers::NONE,
            KeyEventKind::Release,
        ))));
        assert_eq!(pending.page_delta, -1);
    }
}
