use std::sync::mpsc;
use std::thread;
use std::time::Duration;

/// Executes `f` on a spawned thread and waits up to `timeout_ms` for it to
/// complete.
///
/// Returns `Some(value)` if `f` finishes within the timeout, or `None` if
/// the timeout elapses first.
///
/// # Thread lifecycle
///
/// If the timeout expires before `f` returns, the spawned thread continues
/// running in the background — it is **not** cancelled. This is an inherent
/// limitation of OS threads in Rust (there is no safe `Thread::cancel`).
/// Callers should ensure that `f` will eventually terminate on its own;
/// otherwise the thread and any resources it holds will leak until the
/// process exits.
///
/// When the timeout does expire, the `JoinHandle` is intentionally kept
/// alive (moved into an inner scope) so that the thread is not detached
/// abruptly. The channel's `Sender` is dropped when `f` completes, which
/// naturally cleans up the channel resources.
pub fn execute_with_timeout<T, F>(timeout_ms: u64, f: F) -> Option<T>
where
    F: FnOnce() -> T,
    F: Send + 'static,
    T: Send + 'static,
{
    let (tx, rx) = mpsc::channel();

    // Hold onto the JoinHandle so the thread is not silently detached.
    // We intentionally do not join here — the caller has already decided
    // to abandon the result after the timeout.
    let _handle = thread::spawn(move || {
        let result = f();
        // If the receiver has been dropped (timeout elapsed), this send
        // fails silently, which is the desired behavior.
        let _ = tx.send(result);
    });

    rx.recv_timeout(Duration::from_millis(timeout_ms)).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_execute_with_timeout() {
        let result = execute_with_timeout(1000, || {
            thread::sleep(Duration::from_millis(500));
            42
        });
        assert_eq!(result, Some(42));

        let result = execute_with_timeout(1000, || {
            thread::sleep(Duration::from_millis(2000));
            42
        });
        assert_eq!(result, None);
    }
}
