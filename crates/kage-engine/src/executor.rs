//! Cross-thread task marshaller: Tauri Main Thread ──► CEF UI Thread.
//!
//! When `multi_threaded_message_loop = true`, CEF operates its own UI message pump
//! on a dedicated OS thread. Invariant 01 and thread affinity rules require that
//! all direct Chromium browser operations execute strictly on the CEF UI thread.
//!
//! ## Architectural Invariant: Thread-Hop vs. Operation Completion
//! Dispatching a task to the CEF UI thread via `execute` returns the synchronous
//! result of that closure once evaluated on `TID_UI`. However, many CEF operations
//! (e.g., page navigation, DevTools attachment, asynchronous teardown) are inherently
//! asynchronous within Chromium. A successful thread-hop indicates that the call was
//! issued on the correct thread, NOT that Chromium has finished the broader operation.

use crate::errors::CefExecutorError;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, Mutex};

/// Boxed closure representing an asynchronous or synchronous task to run on the CEF UI thread.
pub type BoxedUiTask = Box<dyn FnOnce() + Send + 'static>;

/// Cross-thread executor bridging the Tauri event loop and CEF's UI thread.
#[derive(Clone)]
pub struct CefUiExecutor {
    sender: Arc<Mutex<Option<mpsc::UnboundedSender<BoxedUiTask>>>>,
}

use cef::*;

wrap_task! {
    struct ClosureTask {
        f: Arc<std::sync::Mutex<Option<Box<dyn FnOnce() + Send + 'static>>>>,
    }

    impl Task {
        fn execute(&self) {
            if let Ok(mut guard) = self.f.lock() {
                if let Some(task) = guard.take() {
                    task();
                }
            }
        }
    }
}

impl CefUiExecutor {
    /// Create a new, unattached `CefUiExecutor`.
    pub fn new() -> Self {
        Self {
            sender: Arc::new(Mutex::new(None)),
        }
    }

    /// Attach the executor to an active channel sender connected to the CEF thread.
    pub async fn attach(&self, sender: mpsc::UnboundedSender<BoxedUiTask>) {
        let mut lock = self.sender.lock().await;
        *lock = Some(sender);
    }

    /// Post a fire-and-forget task to execute on the CEF UI thread (`TID_UI`).
    ///
    /// Does not block or wait for completion. Returns an error if the channel
    /// is closed or not attached.
    pub async fn post<F>(&self, task: F) -> Result<(), CefExecutorError>
    where
        F: FnOnce() + Send + 'static,
    {
        let lock = self.sender.lock().await;
        if let Some(ref tx) = *lock {
            tx.send(Box::new(task))
                .map_err(|_| CefExecutorError::ChannelClosed)
        } else {
            Err(CefExecutorError::NotAttached)
        }
    }

    /// Asynchronously dispatch a closure to the CEF UI thread and await its synchronous return value.
    ///
    /// Note: This resolves when the closure finishes executing on the CEF UI thread.
    /// It does NOT wait for any Chromium asynchronous operations initiated inside the closure.
    pub async fn execute<F, R>(&self, task: F) -> Result<R, CefExecutorError>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        let (reply_tx, reply_rx) = oneshot::channel();
        let wrapped_task = move || {
            let result = task();
            let _ = reply_tx.send(result);
        };

        self.post(wrapped_task).await?;

        reply_rx
            .await
            .map_err(|_| CefExecutorError::ExecutionCancelled)
    }

    /// Post a fire-and-forget task directly to CEF's UI thread via `cef::post_task(ThreadId::UI)`.
    ///
    /// (Executor-B / CEF-04B): Uses the native Chromium task runner on `TID_UI`.
    pub fn post_real<F>(&self, task: F) -> Result<(), CefExecutorError>
    where
        F: FnOnce() + Send + 'static,
    {
        let closure = Arc::new(std::sync::Mutex::new(Some(Box::new(task) as Box<dyn FnOnce() + Send + 'static>)));
        let mut cef_task = ClosureTask::new(closure);
        let ret = cef::post_task(ThreadId::UI, Some(&mut cef_task));
        if ret != 1 {
            return Err(CefExecutorError::ChannelClosed);
        }
        Ok(())
    }

    /// Asynchronously execute a closure on CEF's UI thread via `cef::post_task(ThreadId::UI)`
    /// and await its return value.
    pub async fn execute_real<F, R>(&self, task: F) -> Result<R, CefExecutorError>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.post_real(move || {
            let result = task();
            let _ = reply_tx.send(result);
        })?;

        reply_rx
            .await
            .map_err(|_| CefExecutorError::ExecutionCancelled)
    }

    /// Helper to verify whether the current thread is the CEF UI thread (`TID_UI`).
    pub fn currently_on_ui_thread() -> bool {
        cef::currently_on(ThreadId::UI) != 0
    }
}

impl Default for CefUiExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[tokio::test]
    async fn test_executor_fire_and_forget_post() {
        let executor = CefUiExecutor::new();
        let (tx, mut rx) = mpsc::unbounded_channel::<BoxedUiTask>();
        executor.attach(tx).await;

        let executed = Arc::new(AtomicBool::new(false));
        let executed_clone = executed.clone();

        executor
            .post(move || {
                executed_clone.store(true, Ordering::SeqCst);
            })
            .await
            .unwrap();

        // Simulate CEF UI thread processing the task
        let task = rx.recv().await.unwrap();
        task();

        assert!(executed.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_executor_execute_with_return_value() {
        let executor = CefUiExecutor::new();
        let (tx, mut rx) = mpsc::unbounded_channel::<BoxedUiTask>();
        executor.attach(tx).await;

        // Spawn simulated CEF thread worker
        tokio::spawn(async move {
            if let Some(task) = rx.recv().await {
                task();
            }
        });

        let result = executor
            .execute(|| {
                // Executed on simulated CEF thread
                42 * 2
            })
            .await
            .unwrap();

        assert_eq!(result, 84);
    }

    #[tokio::test]
    async fn test_executor_not_attached_error() {
        let executor = CefUiExecutor::new();
        let err = executor.post(|| {}).await.unwrap_err();
        assert_eq!(err, CefExecutorError::NotAttached);
    }
}
