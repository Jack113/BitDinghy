//! Runs the real `dinghy` program, the same way a person would, and checks what it prints.

use std::process::Command;

/// Cargo builds the binary first and tells us where it is via `CARGO_BIN_EXE_<name>`.
fn dinghy() -> Command {
    Command::new(env!("CARGO_BIN_EXE_dinghy"))
}

#[test]
fn version_flag_prints_the_version() {
    let output = dinghy()
        .arg("--version")
        .output()
        .expect("failed to run dinghy");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(env!("CARGO_PKG_VERSION")), "got: {stdout}");
}

#[test]
fn running_with_no_arguments_shows_the_app_name() {
    let output = dinghy().output().expect("failed to run dinghy");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("BitDinghy"), "got: {stdout}");
}
