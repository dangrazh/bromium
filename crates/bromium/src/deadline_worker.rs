//! One bounded activation worker. Caller expiry does not claim COM cancellation.
use std::{
    sync::{
        OnceLock,
        mpsc::{self, SyncSender},
    },
    thread,
    time::Instant,
};
type Operation = Box<dyn FnOnce() -> Result<(), String> + Send>;
struct Job {
    deadline: Instant,
    operation: Operation,
    result: SyncSender<Result<(), String>>,
}
#[derive(Debug)]
pub enum WorkerError {
    Deadline,
    Failed(String),
}
pub fn run(
    deadline: Instant,
    operation: impl FnOnce() -> Result<(), String> + Send + 'static,
) -> Result<(), WorkerError> {
    static QUEUE: OnceLock<SyncSender<Job>> = OnceLock::new();
    if Instant::now() >= deadline {
        return Err(WorkerError::Deadline);
    }
    let queue = QUEUE.get_or_init(|| {
        let (tx, rx) = mpsc::sync_channel::<Job>(8);
        thread::spawn(move || {
            for job in rx {
                if Instant::now() >= job.deadline {
                    continue;
                }
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(job.operation))
                    .unwrap_or_else(|_| Err("activation worker panicked".into()));
                let _ = job.result.try_send(result);
            }
        });
        tx
    });
    let (tx, rx) = mpsc::sync_channel(1);
    queue
        .try_send(Job {
            deadline,
            operation: Box::new(operation),
            result: tx,
        })
        .map_err(|_| WorkerError::Failed("activation capacity unavailable".into()))?;
    rx.recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .map_err(|_| WorkerError::Deadline)?
        .map_err(WorkerError::Failed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        time::Duration,
    };
    #[test]
    fn deadline_bounds_wait_and_expired_work_does_not_start() {
        let started = Arc::new(AtomicBool::new(false));
        let flag = started.clone();
        assert!(matches!(
            run(Instant::now(), move || {
                flag.store(true, Ordering::SeqCst);
                Ok(())
            }),
            Err(WorkerError::Deadline)
        ));
        assert!(!started.load(Ordering::SeqCst));
        let (release, wait) = mpsc::channel();
        let (begun, receive) = mpsc::channel();
        let caller = thread::spawn(move || {
            run(Instant::now() + Duration::from_millis(100), move || {
                begun.send(()).unwrap();
                let _ = wait.recv();
                Ok(())
            })
        });
        receive.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(matches!(caller.join().unwrap(), Err(WorkerError::Deadline)));
        let _ = release.send(());
    }
}
