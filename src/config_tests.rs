use super::*;
use clap::Parser;

#[test]
fn default_matches_clap_empty() {
    let parsed = Config::parse_from(["serial-capture"]);
    assert_eq!(parsed, Config::default());
}

#[test]
fn parse_all_flags() {
    let cfg = Config::parse_from([
        "serial-capture",
        "--device",
        "/dev/ttyUSB0",
        "-d",
        "/dev/ttyACM0",
        "--baud",
        "9600",
        "--text",
        "out.txt",
        "--json",
        "out.json",
        "--csv",
        "out.csv",
        "--poll-ms",
        "250",
        "--list",
    ]);
    assert_eq!(
        cfg.device,
        vec!["/dev/ttyUSB0".to_string(), "/dev/ttyACM0".to_string()]
    );
    assert_eq!(cfg.baud, 9600);
    assert_eq!(cfg.text.as_deref(), Some("out.txt"));
    assert_eq!(cfg.json.as_deref(), Some("out.json"));
    assert_eq!(cfg.csv.as_deref(), Some("out.csv"));
    assert_eq!(cfg.poll_ms, 250);
    assert!(cfg.list);
    assert!(!cfg.all);
    assert!(!cfg.json_nested);
    assert!(!cfg.gpsd);
    assert_eq!(cfg.gpsd_addr, "127.0.0.1:2947");
    assert!(!cfg.gpsd_time);
    assert!(!cfg.dtr);
}

#[test]
fn parse_dtr_flag() {
    let cfg = Config::parse_from(["serial-capture", "--dtr"]);
    assert!(cfg.dtr);
}

#[test]
fn output_defaults_to_stdout_text() {
    let cfg = Config::default().with_output_defaults();
    assert_eq!(cfg.text.as_deref(), Some("-"));
    assert!(cfg.json.is_none());
    assert!(cfg.csv.is_none());
}

#[test]
fn output_defaults_skip_when_list() {
    let cfg = Config {
        list: true,
        ..Config::default()
    }
    .with_output_defaults();
    assert!(cfg.text.is_none());
}

#[test]
fn output_defaults_keep_explicit() {
    let cfg = Config {
        json: Some("a.json".into()),
        ..Config::default()
    }
    .with_output_defaults();
    assert!(cfg.text.is_none());
    assert_eq!(cfg.json.as_deref(), Some("a.json"));
}

#[test]
fn poll_duration_clamps_zero() {
    let cfg = Config {
        poll_ms: 0,
        ..Config::default()
    };
    assert_eq!(
        cfg.poll_duration(),
        std::time::Duration::from_millis(MIN_POLL_MS)
    );
}

#[test]
fn parse_all_and_json_nested() {
    let cfg = Config::parse_from(["serial-capture", "--all", "--json-nested"]);
    assert!(cfg.all);
    assert!(cfg.json_nested);
}

#[test]
fn parse_gpsd_flags() {
    let cfg = Config::parse_from(["serial-capture", "--gpsd", "--gpsd-addr", "10.0.0.5:2947"]);
    assert!(cfg.gpsd);
    assert_eq!(cfg.gpsd_addr, "10.0.0.5:2947");
}

#[test]
fn parse_gpsd_time_requires_gpsd() {
    assert!(Config::try_parse_from(["serial-capture", "--gpsd-time"]).is_err());
    let cfg = Config::parse_from(["serial-capture", "--gpsd", "--gpsd-time"]);
    assert!(cfg.gpsd);
    assert!(cfg.gpsd_time);
}

#[test]
fn rejects_unknown_flag() {
    assert!(Config::try_parse_from(["serial-capture", "--nope"]).is_err());
}
