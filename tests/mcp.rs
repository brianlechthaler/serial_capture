use std::io::Write;
use std::process::{Command, Stdio};

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_serial-capture-mcp"))
}

#[test]
fn ping_over_stdio() {
    let dir = std::env::temp_dir().join(format!("sc-mcp-bin-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut child = bin()
        .env("SERIAL_CAPTURE_LOG_DIR", &dir)
        .env(
            "MCP_AUDIT_PATH",
            dir.join("audit.jsonl").to_string_lossy().as_ref(),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let stdin = child.stdin.as_mut().unwrap();
        stdin
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n")
            .unwrap();
    }
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"id\":1"), "{stdout}");
    std::fs::remove_dir_all(&dir).ok();
}
