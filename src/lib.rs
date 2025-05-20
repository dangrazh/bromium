//! # Bromium
//! 
//! Rust bindings for the Bromium project, a Python library for interacting with the WinDriver API.
//! This module provides a Python interface to the WinDriver API, allowing users to
//! automate tasks and interact with the Windows UI using Python.

mod windriver;
mod context;
mod xpath;
mod bindings;
mod commons;
mod uiauto;
use pyo3::prelude::*;
mod app_control;



/// A Python module implemented in Rust.
#[pymodule]
fn bromium(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<windriver::WinDriver>()?;
    m.add_class::<windriver::Element>()?;
    Ok(())
}
