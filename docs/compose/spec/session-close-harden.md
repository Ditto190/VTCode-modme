---
feature: session-close-harden
status: designed
updated: 2026-09-25
branch: fix/session-close-harden
commits: 29e3da632..
---

# Session Close Harden (Unresponsive UI)

## Report

## [S1] Problem

When a background/foreground exec session exits (or is retained after exit), attempting to stop or close it can hang the main runloop. Observed symptoms:

1. Background task cannot be stopped or closed (`ForceTerminateOrClose` / Ctrl+X appears to do nothing).
2. Main runloop stays in a loading state (`Running PTY command...` shimmer).
3. User cannot send new messages or commands (composer locked).

Root cause: the session-close path performs unbounded blocking waits on the async runloop:

- `PtyManager::close_session` calls `reader_thread.join()` with **no timeout**.
- `PtySessionHandle::graceful_terminate` / `force_terminate` call `child.wait()` while holding the child lock.
- `ExecSessionManager::close_session` awaits aborted watcher tasks and then runs the blocking PTY close **inline on the async worker**.

When any of those waits stall (reader blocked on a PTY `read`, child wait stuck, watcher in a sync section), `close_session` never returns, the runloop cannot process further UI events, and the composer stays locked. `Drop` already uses a timed reader join; `close_session` does not.

## [S2] Design

### Contracts

1. **Bounded waits only.** No unbounded `join()`/`wait()` on the PTY close/terminate path. Reuse `READER_THREAD_TIMEOUT_MS` (5s) for reader join; child wait polls `try_wait` with the same budget instead of `child.wait()`.
2. **Close never stalls the async runtime.** `ExecSessionManager::close_session` runs the blocking backend close on `tokio::task::spawn_blocking`, wrapped in `tokio::time::timeout`.
3. **Counters always release.** Foreground PTY count and background slot are released even when backend close times out or errors. After `force_terminate_or_close` returns (success or timeout), the UI must not keep `Running PTY command...` for that session.
4. **ForceCancel actually kills.** `ForceCancelPtySession` force-terminates active foreground exec sessions and reports the outcome instead of a no-op status line.
5. **Timeout is a soft failure.** On timeout, close returns an error to the caller, but the session record is already detached and counters are released so the UI unblocks. Residual OS process cleanup may finish later; it must not pin the runloop.

### Behavior

- `force_terminate_or_close` on an already-exited session closes it and returns `true` without hanging.
- `force_terminate_or_close` on a live session force-terminates; if terminate hangs past the bound, error is surfaced and counters still drop.
- Graceful terminate of an already-exited session still prints the retained-output message; ForceTerminateOrClose remains the close path and now completes.
- Composer accepts input again once close returns (or times out).

### Testing boundaries

- Unit: timed reader join / child wait do not block past the budget; close releases counters on backend failure.
- Regression: `force_terminate_or_close` completes on a session whose reader thread is stuck (simulated), leaving `active_pty_sessions == 0`.
- No change to wait/yield streaming, background completion capture, or drawer entry retention.

## [S3] Out of Scope

- Processing `ExecSessionAction` concurrently with in-flight tools (drawer mid-turn).
- Changing exec-session retention / local-agents window layout.
- Pipe-backend close paths beyond shared timeout wiring.
- User-visible timeout constants/config.

## Tasks

- [ ] T1: Bound PTY terminate/close waits — acceptance: `graceful_terminate`/`force_terminate` poll `try_wait` under a timeout; `close_session` reader join uses the Drop timeout helper; no unbounded join/wait remains on this path (covers: S2.1)
- [ ] T2: Harden `ExecSessionManager::close_session` — acceptance: backend close runs on `spawn_blocking` with timeout; foreground/background counters release on error/timeout; `force_terminate_or_close` returns without hanging (covers: S2.2, S2.3, S2.5)
- [ ] T3: Implement real ForceCancel — acceptance: `ForceCancelPtySession` force-terminates foreground exec sessions and reports count/outcome (covers: S2.4)
- [ ] T4: Regression tests + verify — acceptance: tests cover stuck-reader close and counter release; `cargo nextest run -p vtcode-core -E 'test(/exec_session/) or test(/pty/)'` and `./scripts/check-dev.sh` pass (covers: S2.1–S2.5)
