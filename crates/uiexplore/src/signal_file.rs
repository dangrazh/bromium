use std::env;
use std::fs::File;
use std::io::{Error, Read, Write};
use std::path::PathBuf;

/// Build a per-process signal-file path so that multiple `uiexplore`
/// instances do not interfere with each other.
fn signal_file_path() -> PathBuf {
    let pid = std::process::id();
    env::temp_dir().join(format!("uiexplore_signal_{}.txt", pid))
}

fn write_to_file(file_name: &PathBuf, text_out: &str) -> Result<(), Error> {
    let mut output = File::create(file_name)?;
    write!(output, "{}", text_out)?;
    Ok(())
}

fn read_to_string(file_name: &PathBuf) -> Result<String, Error> {
    let mut f = File::open(file_name)?;
    let mut buffer = String::new();
    f.read_to_string(&mut buffer)?;
    Ok(buffer)
}

/// Create the signal file so that a child process (`start_screen`) can
/// detect that its parent is alive and has finished initial setup.
pub fn create_signal_file() -> Result<(), Error> {
    write_to_file(&signal_file_path(), "terminate")
}

/// Create a signal file for a specific PID (used by the parent to signal
/// a child process started with a known PID).
pub fn create_signal_file_for_pid(pid: u32) -> Result<(), Error> {
    let path = env::temp_dir().join(format!("uiexplore_signal_{}.txt", pid));
    write_to_file(&path, "terminate")
}

/// Check (and consume) the per-process termination signal.
///
/// Returns `true` exactly once when the signal file exists and contains
/// `"terminate"`, deleting the file in the process.
pub fn termination_signal() -> bool {
    let file_name = signal_file_path();
    if let Ok(text) = read_to_string(&file_name) {
        if text == "terminate" {
            std::fs::remove_file(&file_name).is_ok()
        } else {
            false
        }
    } else {
        false
    }
}
