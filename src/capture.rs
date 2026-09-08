use crate::config::Config;
use crate::device::{targets, Device, Registry, Selector};
use crate::gpsd;
use crate::output::{Gps, GpsPosition, Outputs, Record};
use std::collections::HashSet;
use std::io::{self, Read};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub const READ_TIMEOUT: Duration = Duration::from_millis(100);
pub const MAX_LINE: usize = 1_048_576;
pub const MAX_CAPTURE_THREADS: usize = 32;

#[derive(Default)]
pub struct LineSplitter {
    buf: Vec<u8>,
}

impl LineSplitter {
    pub fn push(&mut self, data: &[u8]) -> Vec<String> {
        self.buf.extend_from_slice(data);
        let mut lines = Vec::new();
        loop {
            if self.buf.len() > MAX_LINE && !self.buf[..MAX_LINE].contains(&b'\n') {
                let chunk: Vec<u8> = self.buf.drain(..MAX_LINE).collect();
                lines.push(String::from_utf8_lossy(&chunk).into_owned());
                continue;
            }
            let Some(index) = self.buf.iter().position(|&b| b == b'\n') else {
                break;
            };
            let mut line: Vec<u8> = self.buf.drain(..=index).collect();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            lines.push(String::from_utf8_lossy(&line).into_owned());
        }
        lines
    }

    pub fn flush(&mut self) -> Option<String> {
        if self.buf.is_empty() {
            None
        } else {
            let line = String::from_utf8_lossy(&self.buf).into_owned();
            self.buf.clear();
            Some(line)
        }
    }
}

pub fn open_serial(path: &str, baud: u32) -> io::Result<Box<dyn Read + Send>> {
    serialport::new(path, baud)
        .timeout(READ_TIMEOUT)
        .open()
        .map(|port| Box::new(port) as Box<dyn Read + Send>)
        .map_err(|err| io::Error::other(err.to_string()))
}

pub(crate) fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|err| err.into_inner())
}

fn gps_snapshot(gps: Option<&Mutex<Option<GpsPosition>>>) -> Gps {
    match gps {
        Some(slot) => Gps::On(*lock(slot)),
        None => Gps::Off,
    }
}

pub(crate) fn emit_lines(
    path: &str,
    lines: impl IntoIterator<Item = String>,
    outputs: &Mutex<Outputs>,
    gps: Option<&Mutex<Option<GpsPosition>>>,
) {
    let gps = gps_snapshot(gps);
    for data in lines {
        let record = Record::with_gps(path, data, gps);
        let _ = lock(outputs).emit(&record);
    }
}

pub fn capture_reader(
    path: &str,
    reader: &mut dyn Read,
    splitter: &mut LineSplitter,
    outputs: &Mutex<Outputs>,
    stop: &AtomicBool,
    gps: Option<&Mutex<Option<GpsPosition>>>,
) -> bool {
    let mut buf = [0u8; 4096];
    loop {
        if stop.load(Ordering::Relaxed) {
            if let Some(line) = splitter.flush() {
                emit_lines(path, [line], outputs, gps);
            }
            return true;
        }
        match reader.read(&mut buf) {
            Ok(0) => {
                if let Some(line) = splitter.flush() {
                    emit_lines(path, [line], outputs, gps);
                }
                return false;
            }
            Ok(n) => emit_lines(path, splitter.push(&buf[..n]), outputs, gps),
            Err(err)
                if err.kind() == io::ErrorKind::TimedOut
                    || err.kind() == io::ErrorKind::WouldBlock =>
            {
                continue;
            }
            Err(_) => {
                if let Some(line) = splitter.flush() {
                    emit_lines(path, [line], outputs, gps);
                }
                return false;
            }
        }
    }
}

pub fn run_loop<L, O, S>(
    cfg: &Config,
    lister: L,
    opener: O,
    outputs: Arc<Mutex<Outputs>>,
    stop: Arc<AtomicBool>,
    sleep: S,
) where
    L: Fn() -> Vec<Device> + Send + Sync + 'static,
    O: Fn(&str, u32) -> io::Result<Box<dyn Read + Send>> + Send + Sync + 'static,
    S: Fn(Duration) + Clone + Send + Sync + 'static,
{
    let selector = Selector::from_devices(&cfg.device);
    let registry = Arc::new(Mutex::new(Registry::default()));
    let opener = Arc::new(opener);
    let mut spawned = HashSet::new();
    let mut handles = Vec::new();
    let poll = cfg.poll_duration();
    let baud = cfg.baud;
    let gps_latest = if cfg.gpsd {
        let latest = Arc::new(Mutex::new(None));
        let thread_latest = latest.clone();
        let thread_stop = stop.clone();
        let thread_sleep = sleep.clone();
        let addr = cfg.gpsd_addr.clone();
        handles.push(thread::spawn(move || {
            gpsd::watch(&addr, &thread_latest, &thread_stop, thread_sleep, poll);
        }));
        Some(latest)
    } else {
        None
    };

    while !stop.load(Ordering::Relaxed) {
        let discovered = lister();
        let next = {
            let mut registry = lock(&registry);
            targets(&selector, &discovered, &mut registry)
        };
        for target in next {
            if spawned.len() >= MAX_CAPTURE_THREADS {
                break;
            }
            if spawned.insert(target.key.clone()) {
                let opener = opener.clone();
                let outputs = outputs.clone();
                let stop_thread = stop.clone();
                let sleep_thread = sleep.clone();
                let registry = registry.clone();
                let gps = gps_latest.clone();
                let key = target.key;
                handles.push(thread::spawn(move || {
                    let mut splitter = LineSplitter::default();
                    while !stop_thread.load(Ordering::Relaxed) {
                        let path = lock(&registry).resolve(&key);
                        match opener(&path, baud) {
                            Ok(mut reader) => {
                                if capture_reader(
                                    &path,
                                    reader.as_mut(),
                                    &mut splitter,
                                    &outputs,
                                    &stop_thread,
                                    gps.as_deref(),
                                ) {
                                    break;
                                }
                            }
                            Err(_) => sleep_thread(poll),
                        }
                    }
                }));
            }
        }
        sleep(poll);
    }

    for handle in handles {
        let _ = handle.join();
    }
}
