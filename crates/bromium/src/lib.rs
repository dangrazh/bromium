//! # Bromium
//! 
//! Rust bindings for the Bromium project, a Python library for interacting with the WinDriver API.
//! This module provides a Python interface to the WinDriver API, allowing users to
//! automate tasks and interact with the Windows UI using Python.

mod macros;

mod windriver;
mod sreen_context;
// mod xpath;
mod commons;
mod uiauto;
use pyo3::prelude::*;
mod app_control;
mod logging;
mod instance_logging;

// pub type UIHashMap<K, V, S = std::hash::RandomState> = std::collections::HashMap<K, V, S>;
// type UIHashSet<T, S = std::hash::RandomState> = std::collections::HashSet<T, S>;

// mod tree_map;
// use tree_map::UITreeMap;

// mod uiexplore;
// use uiexplore::{UITree, UIElementInTree, get_all_elements };

mod rectangle;




/// A Python module implemented in Rust.
#[pymodule]
fn bromium(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Initialize logger on module import
    // logging::init_logger();
    
    m.add_class::<windriver::WinDriver>()?;
    m.add_class::<windriver::Element>()?;
    // m.add_class::<logging::LogLevel>()?;
    // m.add_function(wrap_pyfunction!(logging::set_log_level, m)?)?;
    // m.add_function(wrap_pyfunction!(logging::get_log_level, m)?)?;
    // m.add_function(wrap_pyfunction!(logging::set_log_file, m)?)?;
    // m.add_function(wrap_pyfunction!(logging::set_log_directory, m)?)?;
    // m.add_function(wrap_pyfunction!(logging::get_log_file, m)?)?;
    // m.add_function(wrap_pyfunction!(logging::get_default_log_directory, m)?)?;
    // m.add_function(wrap_pyfunction!(logging::enable_console_logging, m)?)?;
    // m.add_function(wrap_pyfunction!(logging::enable_file_logging, m)?)?;
    // m.add_function(wrap_pyfunction!(logging::reset_log_file, m)?)?;
    Ok(())
}
