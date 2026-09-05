use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_serial-capture"))
}

#[test]
fn list_exits_zero() {
    let output = bin().arg("--list").output().unwrap();
    assert!(output.status.success(), "{:?}", output);
}

#[test]
fn help_exits_zero() {
    let output = bin().arg("--help").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Dump USB serial"));
}

#[test]
fn missing_log_dir_fails() {
    let output = bin()
        .args(["--text", "/this/dir/does/not/exist/capture.log"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.is_empty());
}
