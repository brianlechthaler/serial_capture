use chrono::{DateTime, SecondsFormat, Utc};
use std::fs::OpenOptions;
use std::io::{self, Write};

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub ts: DateTime<Utc>,
    pub device: String,
    pub data: String,
}

impl Record {
    pub fn new(device: impl Into<String>, data: impl Into<String>) -> Self {
        Self {
            ts: Utc::now(),
            device: device.into(),
            data: data.into(),
        }
    }
}

pub fn format_text(record: &Record) -> String {
    format!("{}\t{}\t{}", ts_string(record), record.device, record.data)
}

pub fn format_json(record: &Record) -> String {
    json_line(record, false)
}

pub fn format_json_nested(record: &Record) -> String {
    json_line(record, true)
}

fn json_line(record: &Record, nested: bool) -> String {
    let data = if nested {
        serde_json::from_str(&record.data)
            .unwrap_or_else(|_| serde_json::Value::String(record.data.clone()))
    } else {
        serde_json::Value::String(record.data.clone())
    };
    serde_json::json!({
        "ts": ts_string(record),
        "device": record.device,
        "data": data,
    })
    .to_string()
}

pub fn format_csv(record: &Record) -> String {
    format!(
        "{},{},{}",
        csv_cell(&ts_string(record)),
        csv_cell(&record.device),
        csv_cell(&record.data)
    )
}

pub const CSV_HEADER: &str = "ts,device,data";

fn ts_string(record: &Record) -> String {
    record.ts.to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub fn csv_escape(value: &str) -> String {
    if value.contains(['"', ',', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

pub fn csv_cell(value: &str) -> String {
    let value = if value.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{value}")
    } else {
        value.to_string()
    };
    csv_escape(&value)
}

pub fn open_dest(path: &str) -> io::Result<Box<dyn Write + Send>> {
    if path == "-" {
        Ok(Box::new(io::stdout()))
    } else {
        let mut opts = OpenOptions::new();
        opts.create(true).append(true);
        #[cfg(unix)]
        opts.mode(0o600);
        Ok(Box::new(opts.open(path)?))
    }
}

fn csv_needs_header(path: &str) -> bool {
    if path == "-" {
        return true;
    }
    match std::fs::metadata(path) {
        Ok(meta) => meta.len() == 0,
        Err(_) => true,
    }
}

pub struct Outputs {
    text: Option<Box<dyn Write + Send>>,
    json: Option<Box<dyn Write + Send>>,
    csv: Option<Box<dyn Write + Send>>,
    json_nested: bool,
}

impl Outputs {
    pub fn open(text: Option<&str>, json: Option<&str>, csv: Option<&str>) -> io::Result<Self> {
        Self::open_with_json(text, json, csv, false)
    }

    pub fn open_with_json(
        text: Option<&str>,
        json: Option<&str>,
        csv: Option<&str>,
        json_nested: bool,
    ) -> io::Result<Self> {
        let write_csv_header = csv.map(csv_needs_header).unwrap_or(false);
        let mut outputs = Self {
            text: text.map(open_dest).transpose()?,
            json: json.map(open_dest).transpose()?,
            csv: csv.map(open_dest).transpose()?,
            json_nested,
        };
        if write_csv_header {
            let writer = outputs.csv.as_mut().expect("csv output");
            writeln!(writer, "{CSV_HEADER}")?;
            writer.flush()?;
        }
        Ok(outputs)
    }

    pub fn emit(&mut self, record: &Record) -> io::Result<()> {
        if let Some(writer) = &mut self.text {
            writeln!(writer, "{}", format_text(record))?;
            writer.flush()?;
        }
        if let Some(writer) = &mut self.json {
            let line = if self.json_nested {
                format_json_nested(record)
            } else {
                format_json(record)
            };
            writeln!(writer, "{line}")?;
            writer.flush()?;
        }
        if let Some(writer) = &mut self.csv {
            writeln!(writer, "{}", format_csv(record))?;
            writer.flush()?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "output_tests.rs"]
mod output_tests;
