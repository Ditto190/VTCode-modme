//! Owned tokio task handle with abort-on-drop semantics.
//!
//! Consolidates the ad-hoc `BackgroundTaskGuard`, `SignalHandlerGuard`, and
//! `ProgressUpdateGuard` shapes previously reimplemented per crate: each held
//! an `Option<JoinHandle<()>>` and aborted it on drop. (`TimeoutWarningGuard`
//! is intentionally separate: it drives cooperative cancellation through a
//! `CancellationToken` with an explicit async `cancel()`, not abort-on-drop.)
//! Use this type for new owned background tasks instead of adding another
//! local guard.
//!
//! # Usage
//!
//! ```no_run
//! use vtcode_commons::TaskGuard;
//!
//! let guard = TaskGuard::with_label(tokio::spawn(async {}), "file-palette");
//! // Dropping `guard` aborts the task; `guard.disarm()` releases the handle.
//! ```
//!
//! Detached use remains possible via `disarm()` plus an explicit
//! documented-detached comment at the call site (see "Task Extent, Error
//! Propagation, and Cancel-Safety" in `docs/guides/async-architecture.md`).

use tokio::task::JoinHandle;

/// RAII owner for a spawned tokio task.
///
/// Dropping the guard aborts the task. Await or release the handle with
/// [`Self::disarm`] when the task must outlive the current scope.
#[must_use = "TaskGuard aborts the task when dropped; bind it to a local"]
pub struct TaskGuard {
    handle: Option<JoinHandle<()>>,
    label: &'static str,
}

impl TaskGuard {
    /// Own a spawned task with a default label.
    pub fn new(handle: JoinHandle<()>) -> Self {
        Self::with_label(handle, "task")
    }

    /// Own a spawned task with a static label used for debugging.
    pub fn with_label(handle: JoinHandle<()>, label: &'static str) -> Self {
        Self { handle: Some(handle), label }
    }

    /// Release the handle without aborting, for documented-detached handoff.
    ///
    /// Returns `None` only if the guard was already disarmed.
    pub fn disarm(&mut self) -> Option<JoinHandle<()>> {
        self.handle.take()
    }

    /// Whether the owned task has finished.
    ///
    /// Returns `false` once the guard is disarmed (no handle held), since the
    /// task is no longer owned by this guard.
    pub fn is_finished(&self) -> bool {
        self.handle.as_ref().is_some_and(JoinHandle::is_finished)
    }

    /// Static label identifying the owned task.
    pub fn label(&self) -> &'static str {
        self.label
    }
}

impl std::fmt::Debug for TaskGuard {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TaskGuard")
            .field("label", &self.label)
            .field("finished", &self.is_finished())
            .finish_non_exhaustive()
    }
}

impl Drop for TaskGuard {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    #[tokio::test]
    async fn abort_on_drop_prevents_completion() {
        let completed = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&completed);
        let guard = TaskGuard::with_label(
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(200)).await;
                flag.store(true, Ordering::SeqCst);
            }),
            "abort-on-drop",
        );
        drop(guard);
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(!completed.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn disarm_lets_task_complete() {
        let completed = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&completed);
        let mut guard = TaskGuard::with_label(
            tokio::spawn(async move {
                flag.store(true, Ordering::SeqCst);
            }),
            "disarm",
        );
        let handle = guard.disarm().expect("fresh guard holds a handle");
        assert!(guard.disarm().is_none(), "second disarm releases nothing");
        assert!(!guard.is_finished(), "disarmed guard owns nothing");
        handle.await.expect("disarmed task runs to completion");
        assert!(completed.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn is_finished_distinguishes_completed_and_pending_tasks() {
        let completed = TaskGuard::new(tokio::spawn(async {}));
        // Yield until the empty task completes (bounded wait).
        for _ in 0..100 {
            if completed.is_finished() {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert!(completed.is_finished());

        let pending = TaskGuard::new(tokio::spawn(async {
            tokio::time::sleep(Duration::from_secs(60)).await;
        }));
        assert!(!pending.is_finished());
    }

    #[tokio::test]
    async fn label_defaults_and_debug_render() {
        let guard = TaskGuard::new(tokio::spawn(async {}));
        assert_eq!(guard.label(), "task");
        let rendered = format!("{:?}", guard);
        assert!(rendered.contains("TaskGuard"));
        assert!(rendered.contains("task"));
    }
}
