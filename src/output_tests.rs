use super::*;
use std::fs;

fn rec() -> Record {
    Record {
        ts: DateTime::from_timestamp(1_700_000_000, 123_000_000).unwrap(),
        device: "/dev/ttyUSB0".into(),
        data: "hello".into(),
        gps: Gps::Off,
    }
}

fn rec_gps(pos: Option<GpsPosition>) -> Record {
    Record {
        gps: Gps::On(pos),
        ..rec()
    }
}

#[test]
fn formats_text_json_csv() {
    let record = rec();
    assert_eq!(
        format_text(&record),
        "2023-11-14T22:13:20.123Z\t/dev/ttyUSB0\thello"
    );
    let json = format_json(&record);
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["ts"], "2023-11-14T22:13:20.123Z");
    assert_eq!(value["device"], "/dev/ttyUSB0");
    assert_eq!(value["data"], "hello");
    assert_eq!(
        format_csv(&record),
        "2023-11-14T22:13:20.123Z,/dev/ttyUSB0,hello"
    );
}

#[test]
fn json_keeps_payload_as_string() {
    let record = Record {
        data: r#"{"event":"config","beep_mask":31}"#.into(),
        ..rec()
    };
    let value: serde_json::Value = serde_json::from_str(&format_json(&record)).unwrap();
    assert_eq!(value["data"], r#"{"event":"config","beep_mask":31}"#);
}

#[test]
fn json_nested_parses_payload() {
    let record = Record {
        data: r#"{"event":"config","beep_mask":31}"#.into(),
        ..rec()
    };
    let value: serde_json::Value = serde_json::from_str(&format_json_nested(&record)).unwrap();
    assert_eq!(value["data"]["event"], "config");
    assert_eq!(value["data"]["beep_mask"], 31);
    let text = Record {
        data: "LED on".into(),
        ..rec()
    };
    let value: serde_json::Value = serde_json::from_str(&format_json_nested(&text)).unwrap();
    assert_eq!(value["data"], "LED on");
}

#[test]
fn csv_escapes_specials() {
    assert_eq!(csv_escape("plain"), "plain");
    assert_eq!(csv_escape("a,b"), "\"a,b\"");
    assert_eq!(csv_escape("say \"hi\""), "\"say \"\"hi\"\"\"");
    assert_eq!(csv_escape("a\nb"), "\"a\nb\"");
    assert_eq!(csv_escape("a\rb"), "\"a\rb\"");
    let record = Record {
        data: "x,y".into(),
        ..rec()
    };
    assert!(format_csv(&record).contains("\"x,y\""));
}

#[test]
fn csv_neutralizes_formula_prefix() {
    assert_eq!(csv_cell("=cmd"), "'=cmd");
    assert_eq!(csv_cell("+1+1"), "'+1+1");
    assert_eq!(csv_cell("-1"), "'-1");
    assert_eq!(csv_cell("@SUM(A1)"), "'@SUM(A1)");
    assert_eq!(csv_cell("hello"), "hello");
    let record = Record {
        data: "=2+2".into(),
        ..rec()
    };
    assert!(format_csv(&record).ends_with("'=2+2"));
}

#[test]
fn emit_json_nested_parses_objects() {
    let dir = std::env::temp_dir().join(format!("serial-capture-jsonn-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let json = dir.join("j.json");
    let mut outputs =
        Outputs::open_with_json(None, Some(json.to_str().unwrap()), None, true, false).unwrap();
    outputs
        .emit(&Record {
            data: r#"{"k":1}"#.into(),
            ..rec()
        })
        .unwrap();
    drop(outputs);
    let body = fs::read_to_string(&json).unwrap();
    let value: serde_json::Value = serde_json::from_str(body.trim()).unwrap();
    assert_eq!(value["data"]["k"], 1);
    fs::remove_dir_all(&dir).ok();
}

#[cfg(unix)]
#[test]
fn log_file_mode_is_owner_rw_only() {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("serial-capture-mode-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("secret.txt");
    drop(open_dest(path.to_str().unwrap()).unwrap());
    let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    fs::remove_dir_all(&dir).ok();
    assert_eq!(mode, 0o600);
}

#[test]
fn record_new_sets_fields() {
    let record = Record::new("/dev/ttyUSB1", "ping");
    assert_eq!(record.device, "/dev/ttyUSB1");
    assert_eq!(record.data, "ping");
    assert_eq!(record.gps, Gps::Off);
}

#[test]
fn formats_gps_columns_when_enabled() {
    let record = rec_gps(Some(GpsPosition {
        lat: 37.5,
        lon: -122.25,
        time: None,
    }));
    assert_eq!(
        format_text(&record),
        "2023-11-14T22:13:20.123Z\t/dev/ttyUSB0\t37.5\t-122.25\thello"
    );
    let value: serde_json::Value = serde_json::from_str(&format_json(&record)).unwrap();
    assert_eq!(value["lat"], 37.5);
    assert_eq!(value["lon"], -122.25);
    let nested: serde_json::Value = serde_json::from_str(&format_json_nested(&record)).unwrap();
    assert_eq!(nested["lat"], 37.5);
    assert_eq!(
        format_csv(&record),
        "2023-11-14T22:13:20.123Z,/dev/ttyUSB0,37.5,-122.25,hello"
    );
}

#[test]
fn formats_empty_gps_columns_without_fix() {
    let record = rec_gps(None);
    assert_eq!(
        format_text(&record),
        "2023-11-14T22:13:20.123Z\t/dev/ttyUSB0\t\t\thello"
    );
    let value: serde_json::Value = serde_json::from_str(&format_json(&record)).unwrap();
    assert_eq!(value["lat"], serde_json::Value::Null);
    assert_eq!(value["lon"], serde_json::Value::Null);
    assert_eq!(
        format_csv(&record),
        "2023-11-14T22:13:20.123Z,/dev/ttyUSB0,,,hello"
    );
}

#[test]
fn json_without_gps_omits_lat_lon() {
    let value: serde_json::Value = serde_json::from_str(&format_json(&rec())).unwrap();
    assert!(value.get("lat").is_none());
    assert!(value.get("lon").is_none());
}

#[test]
fn apply_gps_time_overrides_host_clock() {
    let ts = DateTime::from_timestamp(1_700_000_000, 123_000_000).unwrap();
    let mut record = rec_gps(Some(GpsPosition {
        lat: 1.0,
        lon: 2.0,
        time: Some(ts),
    }));
    record.ts = DateTime::from_timestamp(0, 0).unwrap();
    record.apply_gps_time();
    assert_eq!(record.ts, ts);
}

#[test]
fn apply_gps_time_keeps_host_clock_without_fix_time() {
    let host = DateTime::from_timestamp(0, 0).unwrap();
    let mut record = rec_gps(Some(GpsPosition {
        lat: 1.0,
        lon: 2.0,
        time: None,
    }));
    record.ts = host;
    record.apply_gps_time();
    assert_eq!(record.ts, host);
    let mut off = rec();
    off.ts = host;
    off.apply_gps_time();
    assert_eq!(off.ts, host);
    let mut missing = rec_gps(None);
    missing.ts = host;
    missing.apply_gps_time();
    assert_eq!(missing.ts, host);
}

#[test]
fn emit_writes_all_formats_and_csv_header() {
    let dir = std::env::temp_dir().join(format!("serial-capture-out-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let text = dir.join("a.txt");
    let json = dir.join("a.json");
    let csv = dir.join("a.csv");
    let mut outputs = Outputs::open(
        Some(text.to_str().unwrap()),
        Some(json.to_str().unwrap()),
        Some(csv.to_str().unwrap()),
    )
    .unwrap();
    outputs.emit(&rec()).unwrap();
    drop(outputs);
    let text_body = fs::read_to_string(&text).unwrap();
    let json_body = fs::read_to_string(&json).unwrap();
    let csv_body = fs::read_to_string(&csv).unwrap();
    assert!(text_body.contains("hello"));
    assert!(json_body.contains("\"data\":\"hello\""));
    assert!(csv_body.starts_with("ts,device,data\n"));
    assert!(!csv_body.contains("lat,lon"));
    assert!(csv_body.contains("hello"));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn csv_skips_header_when_file_has_bytes() {
    let dir = std::env::temp_dir().join(format!("serial-capture-csv-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let csv = dir.join("b.csv");
    fs::write(&csv, "existing\n").unwrap();
    let mut outputs = Outputs::open(None, None, Some(csv.to_str().unwrap())).unwrap();
    outputs.emit(&rec()).unwrap();
    drop(outputs);
    let body = fs::read_to_string(&csv).unwrap();
    assert!(body.starts_with("existing\n"));
    assert!(!body.contains(CSV_HEADER));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn emit_text_only_and_open_stdout() {
    let mut stdout = open_dest("-").unwrap();
    stdout.write_all(b"").unwrap();
    stdout.flush().unwrap();
    let dir = std::env::temp_dir().join(format!("serial-capture-text-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let text = dir.join("t.txt");
    let mut outputs = Outputs::open(Some(text.to_str().unwrap()), None, None).unwrap();
    outputs.emit(&rec()).unwrap();
    drop(outputs);
    assert!(fs::read_to_string(&text).unwrap().contains("/dev/ttyUSB0"));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn open_missing_parent_fails() {
    assert!(Outputs::open(Some("/no/such/dir/out.txt"), None, None).is_err());
}

#[test]
fn emit_json_only() {
    let dir = std::env::temp_dir().join(format!("serial-capture-json-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let json = dir.join("j.json");
    let mut outputs = Outputs::open(None, Some(json.to_str().unwrap()), None).unwrap();
    outputs.emit(&rec()).unwrap();
    drop(outputs);
    assert!(fs::read_to_string(&json).unwrap().contains("hello"));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn emit_reports_write_errors() {
    let mut outputs = Outputs::open(Some("/dev/full"), None, None).unwrap();
    assert!(outputs.emit(&rec()).is_err());
}

#[test]
fn csv_stdout_header() {
    let mut outputs = Outputs::open(None, None, Some("-")).unwrap();
    outputs.emit(&rec()).unwrap();
}

#[test]
fn csv_gps_header_and_row() {
    let dir = std::env::temp_dir().join(format!("serial-capture-csv-gps-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let csv = dir.join("g.csv");
    let mut outputs =
        Outputs::open_with_json(None, None, Some(csv.to_str().unwrap()), false, true).unwrap();
    outputs
        .emit(&rec_gps(Some(GpsPosition {
            lat: 1.25,
            lon: 2.5,
            time: None,
        })))
        .unwrap();
    drop(outputs);
    let body = fs::read_to_string(&csv).unwrap();
    assert!(body.starts_with("ts,device,lat,lon,data\n"));
    assert!(body.contains("1.25,2.5,hello"));
    fs::remove_dir_all(&dir).ok();
}
