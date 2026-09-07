use super::*;
use std::fs;

fn rec() -> Record {
    Record {
        ts: DateTime::from_timestamp(1_700_000_000, 123_000_000).unwrap(),
        device: "/dev/ttyUSB0".into(),
        data: "hello".into(),
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
fn json_nests_parsed_payload() {
    let record = Record {
        data: r#"{"event":"config","beep_mask":31}"#.into(),
        ..rec()
    };
    let value: serde_json::Value = serde_json::from_str(&format_json(&record)).unwrap();
    assert_eq!(value["data"]["event"], "config");
    assert_eq!(value["data"]["beep_mask"], 31);
    let text = Record {
        data: "LED on".into(),
        ..rec()
    };
    let value: serde_json::Value = serde_json::from_str(&format_json(&text)).unwrap();
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
fn record_new_sets_fields() {
    let record = Record::new("/dev/ttyUSB1", "ping");
    assert_eq!(record.device, "/dev/ttyUSB1");
    assert_eq!(record.data, "ping");
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
