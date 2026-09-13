use crate::agent::runloop::unified::async_mcp_manager::AsyncMcpManager;
use crate::agent::runloop::unified::state::{CtrlCSignal, CtrlCState};
use crate::agent::runloop::unified::stop_requests::request_local_stop;
use std::sync::Arc;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;
use vtcode_core::notifications::set_global_terminal_focused;

/// Owned signal-handler task handle; aborts on drop via the shared guard.
pub(crate) type SignalHandlerGuard = vtcode_commons::TaskGuard;

/// Spawn a signal handler task that listens for SIGINT and SIGTERM.
///
/// # Priority Guarantees
///
/// This signal handler is the highest priority component in the system.
/// It ensures that:
///
/// 1. **SIGINT (Ctrl+C) is always processed immediately** - The handler runs
///    on its own Tokio task and cannot be blocked by other operations.
///
/// 2. **First Ctrl+C cancels current operation** - Calls `request_local_stop()`
///    which transitions `CtrlCState` to `CancelRequested` and notifies waiters.
///
/// 3. **Second Ctrl+C exits the program** - If a second Ctrl+C arrives within
///    1 second, the handler calls `emergency_terminal_cleanup()` which:
///    - Restores the terminal to a usable state
///    - Flushes trace logs
///    - Calls `std::process::exit(130)` to immediately terminate the process
///
/// 4. **Emergency exit bypasses all other operations** - The `std::process::exit(130)`
///    call bypasses Rust's drop logic and async runtime shutdown, ensuring the
///    program exits immediately even if other tasks are blocked.
///
/// 5. **No signal masking** - SIGINT is never blocked or masked, ensuring the
///    OS can always deliver the signal to this handler.
///
/// 6. **MCP shutdown has tight timeout** - On double Ctrl+C, MCP shutdown is
///    awaited inline with a 500ms timeout so MCP children shut down before
///    `std::process::exit(130)`; on the first Ctrl+C (cancel path) it is
///    detached so the handler can process a second Ctrl+C without delay.
///
/// # Signal Flow
///
/// 1. OS delivers SIGINT → Tokio runtime wakes up the signal handler task
/// 2. Signal handler calls `request_local_stop()` → `CtrlCState::register_signal()`
/// 3. If `CtrlCSignal::Exit` is returned → MCP shutdown awaited inline (500ms timeout)
/// 4. Call `emergency_terminal_cleanup()` → `restore_tui()` → `std::process::exit(130)`
///
/// # Emergency Terminal Cleanup
///
/// The `emergency_terminal_cleanup()` function ensures the terminal is left in
/// a usable state even on emergency exit. It:
/// - Disables terminal focus tracking
/// - Restores the TUI to its original state
/// - Flushes trace logs
/// - Terminates the process with exit code 130 (standard SIGINT exit code)
pub(crate) fn spawn_signal_handler(
    ctrl_c_state: Arc<CtrlCState>,
    ctrl_c_notify: Arc<Notify>,
    async_mcp_manager: Option<Arc<AsyncMcpManager>>,
    cancel_token: CancellationToken,
) -> SignalHandlerGuard {
    let handle = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = vtcode_core::shutdown::shutdown_signal() => {
                    let signal = request_local_stop(&ctrl_c_state, &ctrl_c_notify);

                    if matches!(signal, CtrlCSignal::Exit) {
                        // Await the bounded shutdown inline: the spawned variant
                        // never ran, because emergency_terminal_cleanup() calls
                        // std::process::exit(130) before the task could make
                        // progress, orphaning MCP children. Bounded at 500ms so
                        // double-Ctrl+C still exits promptly.
                        if let Some(mcp_manager) = &async_mcp_manager {
                            let _ = tokio::time::timeout(
                                std::time::Duration::from_millis(500),
                                mcp_manager.shutdown(),
                            )
                            .await;
                        }
                        emergency_terminal_cleanup();
                        break;
                    }

                    // Cancel path: deliberately detached, bounded shutdown so
                    // the signal handler loop immediately continues and can
                    // process a second Ctrl+C without delay. Detached is safe
                    // here because the work is bounded (2s) and the process is
                    // not exiting; the outcome is intentionally unobserved.
                    if let Some(mcp_manager) = &async_mcp_manager {
                        let mcp = Arc::clone(mcp_manager);
                        tokio::spawn(async move {
                            let _ = tokio::time::timeout(
                                std::time::Duration::from_secs(2),
                                mcp.shutdown(),
                            ).await;
                        });
                    }
                }
                _ = cancel_token.cancelled() => {
                    break;
                }
            }
        }
    });
    SignalHandlerGuard::new(handle)
}

fn emergency_terminal_cleanup() {
    set_global_terminal_focused(false);
    let _ = vtcode_ui::tui::panic_hook::restore_tui();
    vtcode_commons::trace_flush::flush_trace_log();
    std::process::exit(130);
}
