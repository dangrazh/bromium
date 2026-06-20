#[macro_use]
mod macros;

mod timeout;
pub use timeout::execute_with_timeout;

mod uia;
pub use uia::{RuntimeIdFilter, get_ui_automation_instance};

/// Formats a runtime ID slice as a dash-separated string (e.g., `"42-1-234-56"`).
/// Returns `"0-0-0-0"` if the slice is empty, matching the fallback convention.
pub fn format_runtime_id(id: &[i32]) -> String {
    if id.is_empty() {
        return "0-0-0-0".to_string();
    }
    id.iter()
        .map(|x| x.to_string())
        .collect::<Vec<String>>()
        .join("-")
}
