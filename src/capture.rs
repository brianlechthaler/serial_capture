use crate::config::Config;
use crate::device::{targets, Device, Registry, Selector};
use crate::output::{Outputs, Record};
use std::collections::HashSet;
use std::io::{self, Read};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub const READ_TIMEOUT: Duration = Duration::from_millis(100);

#[derive(Default)]
pub struct LineSplitter {
    buf: Vec<u8>,
}

impl LineSplitter {
    pub fn push(&mut self, data: &[u8]) -> Vec<String> {
        self.buf.extend_from_slice(data);
        let mut lines = Vec::new();
        while let Some(index) = self.buf.iter().position(|&b| b == b'\n') {
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

pub(crate) fn emit_lines(
    path: &str,
    lines: impl IntoIterator<Item = String>,
    outputs: &Mutex<Outputs>,
) {
    for data in lines {
        let record = Record::new(path, data);
        let _ = outputs.lock().map(|mut out| out.emit(&record));
    }
}

pub fn capture_reader(
    path: &str,
    reader: &mut dyn Read,
    splitter: &mut LineSplitter,
    outputs: &Mutex<Outputs>,
    stop: &AtomicBool,
) -> bool {
    let mut buf = [0u8; 4096];
    loop {
        if stop.load(Ordering::Relaxed) {
            if let Some(line) = splitter.flush() {
                emit_lines(path, [line], outputs);
            }
            return true;
        }
        match reader.read(&mut buf) {
            Ok(0) => {
                if let Some(line) = splitter.flush() {
                    emit_lines(path, [line], outputs);
                }
                return false;
            }
            Ok(n) => emit_lines(path, splitter.push(&buf[..n]), outputs),
            Err(err)
                if err.kind() == io::ErrorKind::TimedOut
                    || err.kind() == io::ErrorKind::WouldBlock =>
            {
                continue;
            }
            Err(_) => {
                if let Some(line) = splitter.flush() {
                    emit_lines(path, [line], outputs);
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

    while !stop.load(Ordering::Relaxed) {
        let discovered = lister();
        let next = {
            let mut registry = registry.lock().unwrap();
            targets(&selector, &discovered, &mut registry)
        };
        for target in next {
            if spawned.insert(target.key.clone()) {
                let opener = opener.clone();
                let outputs = outputs.clone();
                let stop_thread = stop.clone();
                let sleep_thread = sleep.clone();
                let registry = registry.clone();
                let key = target.key;
                handles.push(thread::spawn(move || {
                    let mut splitter = LineSplitter::default();
                    while !stop_thread.load(Ordering::Relaxed) {
                        let path = registry.lock().unwrap().resolve(&key);
                        match opener(&path, baud) {
                            Ok(mut reader) => {
                                if capture_reader(
                                    &path,
                                    reader.as_mut(),
                                    &mut splitter,
                                    &outputs,
                                    &stop_thread,
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
