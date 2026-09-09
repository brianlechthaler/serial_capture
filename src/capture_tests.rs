use crate::capture::*;
use crate::config::Config;
use crate::device::{Device, UsbInfo};
use crate::output::Outputs;
use std::collections::{HashSet, VecDeque};
use std::io::{self, Cursor, Read};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

struct Scripted(VecDeque<io::Result<Vec<u8>>>);

impl Read for Scripted {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.steps() {
            None => Ok(0),
            Some(Ok(data)) => {
                let n = data.len().min(buf.len());
                buf[..n].copy_from_slice(&data[..n]);
                Ok(n)
            }
            Some(Err(err)) => Err(err),
        }
    }
}

impl Scripted {
    fn steps(&mut self) -> Option<io::Result<Vec<u8>>> {
        self.0.pop_front()
    }
}

fn wait_until(stop: &AtomicBool, pred: impl Fn() -> bool) {
    wait_until_for(stop, Duration::from_secs(3), pred);
}

fn wait_until_for(stop: &AtomicBool, limit: Duration, pred: impl Fn() -> bool) {
    let start = Instant::now();
    while !pred() {
        if start.elapsed() > limit {
            stop.store(true, Ordering::Relaxed);
            panic!("timed out waiting for capture condition");
        }
        thread::sleep(Duration::from_millis(5));
    }
}

#[test]
#[should_panic(expected = "timed out waiting for capture condition")]
fn wait_until_times_out() {
    let stop = AtomicBool::new(false);
    wait_until_for(&stop, Duration::from_millis(1), || false);
}

#[test]
fn scripted_read_returns_bytes() {
    let outputs = Mutex::new(Outputs::open(None, None, None).unwrap());
    let stop = AtomicBool::new(false);
    let mut splitter = LineSplitter::default();
    let mut reader = Scripted(VecDeque::from([Ok(b"hi\n".to_vec())]));
    assert!(!capture_reader(
        "/dev/ttyUSB0",
        &mut reader,
        &mut splitter,
        &outputs,
        &stop,
        None,
        false
    ));
}

#[test]
fn splitter_caps_line_without_newline() {
    let mut splitter = LineSplitter::default();
    let mut data = vec![b'x'; MAX_LINE + 8];
    data[MAX_LINE + 2] = b'y';
    let lines = splitter.push(&data);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].len(), MAX_LINE);
    assert!(lines[0].chars().all(|c| c == 'x'));
    assert_eq!(splitter.flush().as_deref(), Some("xxyxxxxx"));
}

#[test]
fn splitter_handles_lines_and_flush() {
    let mut splitter = LineSplitter::default();
    assert_eq!(splitter.push(b"hi\n"), vec!["hi".to_string()]);
    assert_eq!(
        splitter.push(b"a\r\nb\nc"),
        vec!["a".to_string(), "b".to_string()]
    );
    assert_eq!(splitter.flush().as_deref(), Some("c"));
    assert!(splitter.flush().is_none());
    assert_eq!(splitter.push(b"\n"), vec!["".to_string()]);
    assert!(splitter.push(b"partial").is_empty());
    assert_eq!(splitter.flush().as_deref(), Some("partial"));
}

#[test]
fn expand_line_skips_empty_and_whitespace() {
    assert!(expand_line("").is_empty());
    assert!(expand_line(" \t ").is_empty());
}

#[test]
fn expand_line_keeps_plain_text_and_panic_dumps() {
    assert_eq!(expand_line("LED on"), vec!["LED on".to_string()]);
    let backtrace = "Backtrace: 0x4205C808:0x3FC9F450 0x4037CC2F:0x3FC9F480";
    assert_eq!(expand_line(backtrace), vec![backtrace.to_string()]);
}

#[test]
fn expand_line_splits_glued_json_objects() {
    assert_eq!(
        expand_line(r#"{"event":"a"}{"event":"b"}"#),
        vec![
            r#"{"event":"a"}"#.to_string(),
            r#"{"event":"b"}"#.to_string()
        ]
    );
}

#[test]
fn expand_line_recovers_json_after_truncated_prefix() {
    assert_eq!(
        expand_line(r#"{"event":"wifi_ap","r{"event":"wifi_ap","ssid":"x"}"#),
        vec![
            r#"{"event":"wifi_ap","r"#.to_string(),
            r#"{"event":"wifi_ap","ssid":"x"}"#.to_string()
        ]
    );
}

#[test]
fn expand_line_keeps_truncated_suffix_after_complete_json() {
    assert_eq!(
        expand_line(r#"{"event":"a"}{"event":"b""#),
        vec![
            r#"{"event":"a"}"#.to_string(),
            r#"{"event":"b""#.to_string()
        ]
    );
}

#[test]
fn expand_line_keeps_single_json_and_truncated_only() {
    assert_eq!(
        expand_line(r#"{"event":"a"}"#),
        vec![r#"{"event":"a"}"#.to_string()]
    );
    assert_eq!(
        expand_line(r#"{"event":"wifi_ap","r"#),
        vec![r#"{"event":"wifi_ap","r"#.to_string()]
    );
}

#[test]
fn expand_line_splits_glued_json_arrays() {
    assert_eq!(
        expand_line("[1][2]"),
        vec!["[1]".to_string(), "[2]".to_string()]
    );
}

#[test]
fn expand_line_skips_space_between_json_values() {
    assert_eq!(
        expand_line(r#"{"event":"a"} {"event":"b"}"#),
        vec![
            r#"{"event":"a"}"#.to_string(),
            r#"{"event":"b"}"#.to_string()
        ]
    );
}

#[test]
fn capture_reader_emits_and_reconnects_on_eof() {
    let dir = std::env::temp_dir().join(format!("serial-capture-read-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("out.txt");
    let outputs = Mutex::new(Outputs::open(Some(path.to_str().unwrap()), None, None).unwrap());
    let stop = AtomicBool::new(false);
    let mut splitter = LineSplitter::default();
    let mut reader = Cursor::new(b"one\ntwo".to_vec());
    assert!(!capture_reader(
        "/dev/ttyUSB0",
        &mut reader,
        &mut splitter,
        &outputs,
        &stop,
        None,
        false
    ));
    drop(outputs);
    let body = std::fs::read_to_string(&path).unwrap();
    assert!(body.contains("one"));
    assert!(body.contains("two"));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn capture_reader_stop_and_timeouts() {
    let outputs = Mutex::new(Outputs::open(None, None, None).unwrap());
    let stop = AtomicBool::new(true);
    let mut splitter = LineSplitter::default();
    splitter.push(b"leftover");
    let mut reader = Scripted(VecDeque::from([Ok(b"x".to_vec())]));
    assert!(capture_reader(
        "/dev/ttyUSB0",
        &mut reader,
        &mut splitter,
        &outputs,
        &stop,
        None,
        false
    ));

    let stop = AtomicBool::new(false);
    let mut splitter = LineSplitter::default();
    let mut reader = Scripted(VecDeque::from([
        Err(io::Error::new(io::ErrorKind::TimedOut, "t")),
        Err(io::Error::new(io::ErrorKind::WouldBlock, "w")),
        Err(io::Error::other("boom")),
    ]));
    splitter.push(b"tail");
    assert!(!capture_reader(
        "/dev/ttyUSB0",
        &mut reader,
        &mut splitter,
        &outputs,
        &stop,
        None,
        false
    ));
}

#[test]
fn run_loop_reconnects_and_discovers() {
    let dir = std::env::temp_dir().join(format!("serial-capture-loop-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let log = dir.join("cap.txt");
    let outputs = Arc::new(Mutex::new(
        Outputs::open(Some(log.to_str().unwrap()), None, None).unwrap(),
    ));
    let stop = Arc::new(AtomicBool::new(false));
    let opens = Arc::new(AtomicUsize::new(0));
    let ticks = Arc::new(AtomicUsize::new(0));
    let cfg = Config {
        poll_ms: 1,
        ..Config::default()
    };
    let lister_ticks = ticks.clone();
    let opener_opens = opens.clone();
    let thread_stop = stop.clone();
    let handle = thread::spawn(move || {
        run_loop(
            &cfg,
            move || {
                if lister_ticks.load(Ordering::Relaxed) == 0 {
                    Vec::new()
                } else {
                    vec![Device::from_path("/dev/ttyUSB0")]
                }
            },
            move |_, _, _| {
                opener_opens.fetch_add(1, Ordering::Relaxed);
                Ok(Box::new(Cursor::new(b"hello\n".to_vec())) as Box<dyn Read + Send>)
            },
            outputs,
            thread_stop,
            {
                let ticks = ticks.clone();
                move |_| {
                    ticks.fetch_add(1, Ordering::Relaxed);
                }
            },
        );
    });
    wait_until(&stop, || opens.load(Ordering::Relaxed) >= 2);
    stop.store(true, Ordering::Relaxed);
    handle.join().unwrap();
    let body = std::fs::read_to_string(&log).unwrap();
    assert!(body.contains("hello"));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn run_loop_specified_device_remaps_path() {
    let dir = std::env::temp_dir().join(format!("serial-capture-map-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let log = dir.join("cap.txt");
    let outputs = Arc::new(Mutex::new(
        Outputs::open(Some(log.to_str().unwrap()), None, None).unwrap(),
    ));
    let stop = Arc::new(AtomicBool::new(false));
    let last_path = Arc::new(Mutex::new(String::new()));
    let ticks = Arc::new(AtomicUsize::new(0));
    let usb = UsbInfo {
        vid: 1,
        pid: 2,
        serial: Some("SN".into()),
        manufacturer: None,
        product: None,
    };
    let cfg = Config {
        device: vec!["/dev/ttyUSB0".into()],
        poll_ms: 1,
        ..Config::default()
    };
    let lister_ticks = ticks.clone();
    let usb_first = usb.clone();
    let usb_second = usb;
    let seen = last_path.clone();
    let thread_stop = stop.clone();
    let handle = thread::spawn(move || {
        run_loop(
            &cfg,
            move || {
                if lister_ticks.load(Ordering::Relaxed) < 2 {
                    vec![Device {
                        path: "/dev/ttyUSB0".into(),
                        usb: Some(usb_first.clone()),
                    }]
                } else {
                    vec![Device {
                        path: "/dev/ttyUSB1".into(),
                        usb: Some(usb_second.clone()),
                    }]
                }
            },
            {
                let seen = seen.clone();
                move |path, _, _| {
                    *seen.lock().unwrap() = path.to_string();
                    Ok(Box::new(Cursor::new(b"x\n".to_vec())) as Box<dyn Read + Send>)
                }
            },
            outputs,
            thread_stop,
            {
                let ticks = ticks.clone();
                move |_| {
                    ticks.fetch_add(1, Ordering::Relaxed);
                }
            },
        );
    });
    wait_until(&stop, || {
        last_path.lock().unwrap().as_str() == "/dev/ttyUSB1"
    });
    stop.store(true, Ordering::Relaxed);
    handle.join().unwrap();
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn run_loop_retries_open_errors() {
    let outputs = Arc::new(Mutex::new(Outputs::open(None, None, None).unwrap()));
    let stop = Arc::new(AtomicBool::new(false));
    let opens = Arc::new(AtomicUsize::new(0));
    let cfg = Config {
        device: vec!["/dev/ttyUSB0".into()],
        poll_ms: 1,
        dtr: true,
        ..Config::default()
    };
    let opener_opens = opens.clone();
    let thread_stop = stop.clone();
    let handle = thread::spawn(move || {
        run_loop(
            &cfg,
            Vec::new,
            move |_, _, dtr| {
                assert!(dtr);
                opener_opens.fetch_add(1, Ordering::Relaxed);
                Err(io::Error::other("missing"))
            },
            outputs,
            thread_stop,
            |_| {},
        );
    });
    wait_until(&stop, || opens.load(Ordering::Relaxed) >= 2);
    stop.store(true, Ordering::Relaxed);
    handle.join().unwrap();
}

#[test]
fn open_serial_missing_path_fails() {
    assert!(open_serial("/dev/ttyUSB-does-not-exist-xyz", 115200, false).is_err());
    assert!(open_serial("/dev/ttyUSB-does-not-exist-xyz", 115200, true).is_err());
}

#[test]
fn emit_lines_skips_empty_and_splits_glued_json() {
    let dir = std::env::temp_dir().join(format!("serial-capture-expand-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("out.json");
    let outputs = Mutex::new(
        Outputs::open_with_json(None, Some(path.to_str().unwrap()), None, true, false).unwrap(),
    );
    emit_lines(
        "/dev/ttyACM0",
        [
            String::new(),
            "   ".into(),
            r#"{"event":"a"}{"event":"b"}"#.into(),
        ],
        &outputs,
        None,
        false,
    );
    drop(outputs);
    let body = std::fs::read_to_string(&path).unwrap();
    let rows: Vec<serde_json::Value> = body
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["data"]["event"], "a");
    assert_eq!(rows[1]["data"]["event"], "b");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn capture_reader_empty_flush_on_stop_eof_and_error() {
    let outputs = Mutex::new(Outputs::open(None, None, None).unwrap());
    let mut splitter = LineSplitter::default();
    let stop = AtomicBool::new(true);
    assert!(capture_reader(
        "/dev/ttyUSB0",
        &mut Cursor::new(Vec::<u8>::new()),
        &mut splitter,
        &outputs,
        &stop,
        None,
        false
    ));

    let stop = AtomicBool::new(false);
    assert!(!capture_reader(
        "/dev/ttyUSB0",
        &mut Cursor::new(Vec::<u8>::new()),
        &mut splitter,
        &outputs,
        &stop,
        None,
        false
    ));

    let mut reader = Scripted(VecDeque::from([Err(io::Error::other("boom"))]));
    assert!(!capture_reader(
        "/dev/ttyUSB0",
        &mut reader,
        &mut splitter,
        &outputs,
        &stop,
        None,
        false
    ));
}

#[test]
fn lock_recovers_poisoned_mutex() {
    let mutex = std::sync::Mutex::new(7);
    let poisoned = std::sync::Arc::new(mutex);
    let clone = poisoned.clone();
    let _ = thread::spawn(move || {
        let _guard = clone.lock().unwrap();
        panic!("poison");
    })
    .join();
    assert_eq!(*lock(poisoned.as_ref()), 7);
}

#[test]
fn run_loop_caps_capture_threads() {
    let outputs = Arc::new(Mutex::new(Outputs::open(None, None, None).unwrap()));
    let stop = Arc::new(AtomicBool::new(false));
    let seen = Arc::new(Mutex::new(HashSet::new()));
    let cfg = Config {
        poll_ms: 1,
        ..Config::default()
    };
    let opener_seen = seen.clone();
    let thread_stop = stop.clone();
    let handle = thread::spawn(move || {
        run_loop(
            &cfg,
            || {
                (0..=MAX_CAPTURE_THREADS)
                    .map(|i| Device::from_path(format!("/dev/ttyUSB{i}")))
                    .collect()
            },
            move |path, _, _| {
                opener_seen.lock().unwrap().insert(path.to_string());
                Err(io::Error::other("missing"))
            },
            outputs,
            thread_stop,
            |_| {},
        );
    });
    wait_until(&stop, || seen.lock().unwrap().len() >= MAX_CAPTURE_THREADS);
    thread::sleep(Duration::from_millis(30));
    stop.store(true, Ordering::Relaxed);
    handle.join().unwrap();
    assert_eq!(seen.lock().unwrap().len(), MAX_CAPTURE_THREADS);
}

#[test]
fn emit_lines_survives_poisoned_mutex() {
    let outputs = Arc::new(Mutex::new(Outputs::open(None, None, None).unwrap()));
    let poisoned = outputs.clone();
    let _ = thread::spawn(move || {
        let _guard = poisoned.lock().unwrap();
        panic!("poison");
    })
    .join();
    emit_lines("/dev/ttyUSB0", ["x".into()], &outputs, None, false);
}

#[test]
fn emit_lines_adds_gps_columns() {
    let dir = std::env::temp_dir().join(format!("serial-capture-gps-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("out.txt");
    let outputs = Mutex::new(
        Outputs::open_with_json(Some(path.to_str().unwrap()), None, None, false, true).unwrap(),
    );
    let gps = Mutex::new(Some(crate::output::GpsPosition {
        lat: 10.5,
        lon: -20.25,
        time: None,
    }));
    emit_lines("/dev/ttyUSB0", ["fix".into()], &outputs, Some(&gps), false);
    drop(outputs);
    let body = std::fs::read_to_string(&path).unwrap();
    assert!(body.contains("10.5"));
    assert!(body.contains("-20.25"));
    assert!(body.contains("fix"));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn emit_lines_uses_gps_time_when_enabled() {
    let dir = std::env::temp_dir().join(format!("serial-capture-gps-time-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("out.txt");
    let outputs = Mutex::new(
        Outputs::open_with_json(Some(path.to_str().unwrap()), None, None, false, true).unwrap(),
    );
    let ts = chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap();
    let gps = Mutex::new(Some(crate::output::GpsPosition {
        lat: 1.0,
        lon: 2.0,
        time: Some(ts),
    }));
    emit_lines("/dev/ttyUSB0", ["tick".into()], &outputs, Some(&gps), true);
    drop(outputs);
    let body = std::fs::read_to_string(&path).unwrap();
    assert!(body.contains("2023-11-14T22:13:20.000Z"));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn run_loop_starts_gpsd_watcher() {
    let dir = std::env::temp_dir().join(format!("serial-capture-gpsd-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let log = dir.join("cap.txt");
    let outputs = Arc::new(Mutex::new(
        Outputs::open_with_json(Some(log.to_str().unwrap()), None, None, false, true).unwrap(),
    ));
    let stop = Arc::new(AtomicBool::new(false));
    let opens = Arc::new(AtomicUsize::new(0));
    let cfg = Config {
        gpsd: true,
        gpsd_time: true,
        gpsd_addr: "127.0.0.1:1".into(),
        poll_ms: 1,
        device: vec!["/dev/ttyUSB0".into()],
        ..Config::default()
    };
    let opener_opens = opens.clone();
    let thread_stop = stop.clone();
    let handle = thread::spawn(move || {
        run_loop(
            &cfg,
            Vec::new,
            move |_, _, _| {
                opener_opens.fetch_add(1, Ordering::Relaxed);
                Ok(Box::new(Cursor::new(b"hello\n".to_vec())) as Box<dyn Read + Send>)
            },
            outputs,
            thread_stop,
            |_| {},
        );
    });
    wait_until(&stop, || opens.load(Ordering::Relaxed) >= 1);
    stop.store(true, Ordering::Relaxed);
    handle.join().unwrap();
    let body = std::fs::read_to_string(&log).unwrap();
    assert!(body.contains("hello"));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn run_loop_skips_duplicate_keys() {
    let outputs = Arc::new(Mutex::new(Outputs::open(None, None, None).unwrap()));
    let stop = Arc::new(AtomicBool::new(false));
    let ticks = Arc::new(AtomicUsize::new(0));
    let cfg = Config {
        device: vec!["/dev/ttyUSB0".into(), "/dev/ttyUSB0".into()],
        poll_ms: 1,
        ..Config::default()
    };
    let thread_stop = stop.clone();
    let opens = ticks.clone();
    let handle = thread::spawn(move || {
        run_loop(
            &cfg,
            Vec::new,
            {
                let opens = opens.clone();
                move |_, _, _| {
                    opens.fetch_add(1, Ordering::Relaxed);
                    Err(io::Error::other("missing"))
                }
            },
            outputs,
            thread_stop,
            |_| {},
        );
    });
    wait_until(&stop, || ticks.load(Ordering::Relaxed) >= 1);
    stop.store(true, Ordering::Relaxed);
    handle.join().unwrap();
}

#[test]
fn emit_lines_ignores_write_errors() {
    let outputs = Mutex::new(Outputs::open(Some("/dev/full"), None, None).unwrap());
    let stop = AtomicBool::new(false);
    let mut splitter = LineSplitter::default();
    let mut reader = Cursor::new(b"line\n".to_vec());
    assert!(!capture_reader(
        "/dev/ttyUSB0",
        &mut reader,
        &mut splitter,
        &outputs,
        &stop,
        None,
        false
    ));
}

#[cfg(unix)]
#[test]
fn open_serial_pty_succeeds() {
    unsafe {
        let master = libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY);
        assert!(master >= 0);
        assert_eq!(libc::grantpt(master), 0);
        assert_eq!(libc::unlockpt(master), 0);
        let mut buf = [0 as libc::c_char; 64];
        assert_eq!(libc::ptsname_r(master, buf.as_mut_ptr(), buf.len()), 0);
        let name = std::ffi::CStr::from_ptr(buf.as_ptr().cast())
            .to_string_lossy()
            .into_owned();
        open_serial(&name, 115200, false).expect("pty slave should open with DTR off");
        let on = open_serial(&name, 115200, true);
        libc::close(master);
        on.expect("pty slave should open with DTR on");
    }
}

fn pos() -> crate::output::GpsPosition {
    crate::output::GpsPosition {
        lat: 1.0,
        lon: 2.0,
        time: None,
    }
}

#[test]
fn wait_gpsd_ready_returns_immediately_with_fix() {
    let latest = Mutex::new(Some(pos()));
    let stop = AtomicBool::new(false);
    wait_gpsd_ready(
        &latest,
        &stop,
        thread::sleep,
        GPSD_READY_WAIT,
        GPSD_READY_STEP,
    );
    assert_eq!(*lock(&latest), Some(pos()));
}

#[test]
fn wait_gpsd_ready_skips_when_stopped() {
    let latest = Mutex::new(None);
    let stop = AtomicBool::new(true);
    wait_gpsd_ready(
        &latest,
        &stop,
        thread::sleep,
        GPSD_READY_WAIT,
        GPSD_READY_STEP,
    );
    assert!(lock(&latest).is_none());
}

#[test]
fn wait_gpsd_ready_stops_when_fix_arrives() {
    let latest = Mutex::new(None);
    let stop = AtomicBool::new(false);
    wait_gpsd_ready(
        &latest,
        &stop,
        |_| {
            *lock(&latest) = Some(pos());
        },
        GPSD_READY_WAIT,
        GPSD_READY_STEP,
    );
    assert_eq!(*lock(&latest), Some(pos()));
}

#[test]
fn wait_gpsd_ready_times_out_without_fix() {
    let latest = Mutex::new(None);
    let stop = AtomicBool::new(false);
    let sleeps = AtomicUsize::new(0);
    wait_gpsd_ready(
        &latest,
        &stop,
        |_| {
            sleeps.fetch_add(1, Ordering::Relaxed);
        },
        Duration::from_millis(100),
        Duration::from_millis(50),
    );
    assert_eq!(sleeps.load(Ordering::Relaxed), 2);
    assert!(lock(&latest).is_none());
}
