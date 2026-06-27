#![deny(unsafe_op_in_unsafe_fn)]

//! # Bromium
//!
//! Rust bindings for the Bromium project, a Python library for interacting with the WinDriver API.
//! This module provides a Python interface to the WinDriver API, allowing users to
//! automate tasks and interact with the Windows UI using Python.

mod app_control;
pub mod exceptions;
mod logging;
mod rectangle;
mod screen_context;
mod uiauto;
mod windriver;

use pyo3::prelude::*;

/// A Python module implemented in Rust.
#[pymodule]
fn bromium(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Classes
    m.add_class::<windriver::Bromium>()?;
    m.add_class::<windriver::WinDriver>()?;
    m.add_class::<windriver::Element>()?;
    m.add_class::<windriver::ElementIterator>()?;
    m.add_class::<screen_context::ScreenContext>()?;
    m.add_class::<screen_context::ScreenInfo>()?;
    m.add_class::<logging::LogLevel>()?;

    // Custom exceptions
    m.add(
        "ElementNotFoundError",
        m.py().get_type::<exceptions::ElementNotFoundError>(),
    )?;
    m.add(
        "AutomationError",
        m.py().get_type::<exceptions::AutomationError>(),
    )?;
    m.add(
        "TreeConstructionError",
        m.py().get_type::<exceptions::TreeConstructionError>(),
    )?;

    // Module-level functions (R-02: mirrors Bromium static methods)
    m.add_function(wrap_pyfunction!(logging::py_init_logging, m)?)?;
    m.add_function(wrap_pyfunction!(logging::py_get_version, m)?)?;
    m.add_function(wrap_pyfunction!(logging::py_get_log_file, m)?)?;
    m.add_function(wrap_pyfunction!(logging::py_set_log_file, m)?)?;
    m.add_function(wrap_pyfunction!(logging::py_get_log_level, m)?)?;
    m.add_function(wrap_pyfunction!(logging::py_set_log_level, m)?)?;
    m.add_function(wrap_pyfunction!(logging::py_set_log_directory, m)?)?;
    m.add_function(wrap_pyfunction!(logging::py_enable_console_logging, m)?)?;
    m.add_function(wrap_pyfunction!(logging::py_enable_file_logging, m)?)?;
    m.add_function(wrap_pyfunction!(logging::py_reset_log_file, m)?)?;

    Ok(())
}
