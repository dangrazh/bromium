//! Tiny executable adapter for the argument-free Python launch API.
//! Only used by opt-in tests; all inputs point to a test-owned session.
#![windows_subsystem = "windows"]
use std::{
    env,
    os::windows::process::CommandExt,
    process::{Command, Stdio},
};

fn main() {
    let python = env::var_os("BROMIUM_FIXTURE_PYTHON").expect("test interpreter required");
    let script = env::var_os("BROMIUM_FIXTURE_SCRIPT").expect("test fixture required");
    let session = env::var_os("BROMIUM_FIXTURE_SESSION").expect("test session required");
    assert!(
        std::path::Path::new(&session).is_dir(),
        "session must exist"
    );
    let status = Command::new(python)
        .arg("-u")
        .arg(script)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(0x08000000)
        .status()
        .expect("fixture launch failed");
    std::process::exit(status.code().unwrap_or(1));
}
