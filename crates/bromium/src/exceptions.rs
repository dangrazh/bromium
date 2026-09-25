//! Custom Python exception types for the bromium module.
//!
//! These provide targeted `except` clauses for Python developers,
//! replacing generic `ValueError`/`RuntimeError` with domain-specific errors.

use pyo3::create_exception;
use pyo3::exceptions::{PyException, PyTimeoutError};

// ElementNotFoundError — raised when a UI element cannot be located
// (by xpath, coordinates, or runtime ID).
create_exception!(bromium, ElementNotFoundError, PyException);

// AutomationError — raised when a UI Automation operation fails
// (click, send_keys, set_value, etc.).
create_exception!(bromium, AutomationError, PyException);

// TreeConstructionError — raised when the UI tree cannot be built or refreshed
// (COM failures, channel timeouts, XML errors).
create_exception!(bromium, TreeConstructionError, PyTimeoutError);
create_exception!(bromium, StaleTreeError, PyTimeoutError);

// All library-raised domain exceptions go through this constructor. Keeping the
// log at the Python boundary avoids logging expected internal retries as errors.
macro_rules! logged_exception {
    ($($exception:ident),+ $(,)?) => { $(
        impl $exception {
            #[track_caller]
            pub(crate) fn logged_err(message: impl Into<String>) -> pyo3::PyErr {
                let message = message.into();
                log::error!("{}: {} (raised at {})", stringify!($exception), message, std::panic::Location::caller());
                Self::new_err(message)
            }
        }
    )+ };
}
logged_exception!(
    ElementNotFoundError,
    AutomationError,
    TreeConstructionError,
    StaleTreeError
);

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct Recorder(Mutex<Vec<(std::thread::ThreadId, log::Level, String)>>);
    static RECORDER: Recorder = Recorder(Mutex::new(Vec::new()));
    impl log::Log for Recorder {
        fn enabled(&self, _: &log::Metadata<'_>) -> bool {
            true
        }
        fn log(&self, record: &log::Record<'_>) {
            self.0.lock().unwrap().push((
                std::thread::current().id(),
                record.level(),
                record.args().to_string(),
            ));
        }
        fn flush(&self) {}
    }

    #[test]
    fn every_domain_exception_logs_details_at_error_level() {
        log::set_logger(&RECORDER).unwrap();
        log::set_max_level(log::LevelFilter::Error);
        let _ = ElementNotFoundError::logged_err("missing runtime ID 42");
        let _ = AutomationError::logged_err("provider rejected action");
        let _ = TreeConstructionError::logged_err("initial capture failed");
        let _ = StaleTreeError::logged_err("scope=None revision=95 coverage=dirty");
        let records = RECORDER.0.lock().unwrap();
        let messages: Vec<_> = records
            .iter()
            .filter(|(thread, _, _)| *thread == std::thread::current().id())
            .collect();
        assert_eq!(messages.len(), 4);
        for ((_, level, message), expected) in messages.into_iter().zip([
            "ElementNotFoundError: missing runtime ID 42",
            "AutomationError: provider rejected action",
            "TreeConstructionError: initial capture failed",
            "StaleTreeError: scope=None revision=95 coverage=dirty",
        ]) {
            assert_eq!(*level, log::Level::Error);
            assert!(message.starts_with(expected));
            assert!(message.contains("exceptions.rs:"));
        }
    }
}
