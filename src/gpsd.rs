use crate::output::GpsPosition;
use std::io::{self, BufRead, BufReader, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

const WATCH: &str = r#"?WATCH={"enable":true,"json":true}"#;
const READ_TIMEOUT: Duration = Duration::from_millis(100);

pub fn parse_tpv(line: &str) -> Option<GpsPosition> {
    let value: serde_json::Value = serde_json::from_str(line.trim()).ok()?;
    if value.get("class").and_then(serde_json::Value::as_str) != Some("TPV") {
        return None;
    }
    Some(GpsPosition {
        lat: value.get("lat").and_then(serde_json::Value::as_f64)?,
        lon: value.get("lon").and_then(serde_json::Value::as_f64)?,
    })
}

pub fn watch(
    addr: &str,
    latest: &Mutex<Option<GpsPosition>>,
    stop: &AtomicBool,
    sleep: impl Fn(Duration),
    poll: Duration,
) {
    while !stop.load(Ordering::Relaxed) {
        match connect(addr, poll) {
            Ok(stream) => read_fixes(stream, latest, stop),
            Err(_) => sleep(poll),
        }
    }
}

pub fn connect(addr: &str, timeout: Duration) -> io::Result<TcpStream> {
    let mut addrs = addr.to_socket_addrs()?;
    let sock = match addrs.next() {
        Some(sock) => sock,
        None => return Err(io::Error::other("gpsd address has no IPs")),
    };
    let stream = TcpStream::connect_timeout(&sock, timeout)?;
    stream.set_read_timeout(Some(READ_TIMEOUT))?;
    stream.set_write_timeout(Some(READ_TIMEOUT))?;
    Ok(stream)
}

fn read_fixes(mut stream: TcpStream, latest: &Mutex<Option<GpsPosition>>, stop: &AtomicBool) {
    if writeln!(stream, "{WATCH}").is_err() {
        return;
    }
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    while !stop.load(Ordering::Relaxed) {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => return,
            Ok(_) => {
                if let Some(pos) = parse_tpv(&line) {
                    *lock(latest) = Some(pos);
                }
            }
            Err(err)
                if err.kind() == io::ErrorKind::TimedOut
                    || err.kind() == io::ErrorKind::WouldBlock =>
            {
                continue;
            }
            Err(_) => return,
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|err| err.into_inner())
}

#[cfg(test)]
#[path = "gpsd_tests.rs"]
mod gpsd_tests;
