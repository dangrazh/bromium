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
