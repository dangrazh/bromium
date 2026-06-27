#![deny(unsafe_op_in_unsafe_fn)]

mod timeout;
pub use timeout::execute_with_timeout;

mod uia;
pub use uia::{RuntimeIdFilter, get_ui_automation_instance};

pub mod rectangle;

/// Formats a runtime ID slice as a dash-separated string (e.g., `"42-1-234-56"`).
/// Returns `"0-0-0-0"` if the slice is empty, matching the fallback convention.
pub fn format_runtime_id(id: &[i32]) -> String {
    join_runtime_id(id, '-')
}

fn join_runtime_id(id: &[i32], sep: char) -> String {
    use std::fmt::Write;
    if id.is_empty() {
        return "0-0-0-0".to_string();
    }
    let mut s = String::with_capacity(id.len() * 5);
    for (i, val) in id.iter().enumerate() {
        if i > 0 {
            s.push(sep);
        }
        let _ = write!(s, "{}", val);
    }
    s
}

/// Formats a runtime ID slice as a dot-separated string (e.g., `"42.1.234.56"`).
pub fn format_runtime_id_dotted(id: &[i32]) -> String {
    join_runtime_id(id, '.')
}
