use super::*;

#[test]
fn run_requires_device_or_all() {
    assert!(run(Config::default()).is_err());
}

#[test]
fn run_list_succeeds() {
    let cfg = Config {
        list: true,
        ..Config::default()
    };
    run(cfg).unwrap();
}

#[test]
fn run_missing_output_dir_fails() {
    let cfg = Config {
        text: Some("/no/such/serial-capture-dir/out.txt".into()),
        all: true,
        ..Config::default()
    };
    assert!(run(cfg).is_err());
}

#[test]
fn run_with_stop_already_set_returns() {
    let dir = std::env::temp_dir().join(format!("serial-capture-run-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let text = dir.join("out.txt");
    let cfg = Config {
        text: Some(text.to_string_lossy().into_owned()),
        poll_ms: 1,
        device: vec!["/dev/ttyUSB-missing-for-test".into()],
        ..Config::default()
    };
    run_with(cfg, Arc::new(AtomicBool::new(true))).unwrap();
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn run_with_gpsd_stop_already_set_returns() {
    let dir = std::env::temp_dir().join(format!("serial-capture-run-gpsd-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let text = dir.join("out.txt");
    let cfg = Config {
        text: Some(text.to_string_lossy().into_owned()),
        poll_ms: 1,
        device: vec!["/dev/ttyUSB-missing-for-test".into()],
        gpsd: true,
        gpsd_addr: "127.0.0.1:1".into(),
        ..Config::default()
    };
    run_with(cfg, Arc::new(AtomicBool::new(true))).unwrap();
    std::fs::remove_dir_all(&dir).ok();
}
