//! # Bromium
//!
//! Rust bindings for the Bromium project, a Python library for interacting with the WinDriver API.
//! This module provides a Python interface to the WinDriver API, allowing users to
//! automate tasks and interact with the Windows UI using Python.

mod macros;

mod commons;
mod screen_context;
mod uiauto;
mod windriver;
use pyo3::prelude::*;
mod app_control;
mod instance_logging;
mod logging;

mod rectangle;

/// A Python module implemented in Rust.
#[pymodule]
fn bromium(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<windriver::Bromium>()?;
    m.add_class::<windriver::WinDriver>()?;
    m.add_class::<windriver::Element>()?;
    Ok(())
}
