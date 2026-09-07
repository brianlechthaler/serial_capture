use crate::capture::{open_serial, run_loop};
use crate::device::{list_devices, write_devices};
use crate::output::Outputs;
use std::io;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::thread;

pub mod capture;
pub mod config;
pub mod device;
pub mod output;

pub use config::Config;
pub use device::Device;
pub use output::{csv_cell, format_csv, format_json, format_json_nested, format_text, Record};

pub type AppResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub fn run(cfg: Config) -> AppResult<()> {
    run_with(cfg, Arc::new(AtomicBool::new(false)))
}

pub fn run_with(cfg: Config, stop: Arc<AtomicBool>) -> AppResult<()> {
    let cfg = cfg.with_output_defaults();
    if cfg.list {
        write_devices(&list_devices(), &mut io::stdout())?;
        return Ok(());
    }
    if cfg.device.is_empty() && !cfg.all {
        return Err("specify --device PATH or --all to capture USB serial devices".into());
    }
    let outputs = Outputs::open_with_json(
        cfg.text.as_deref(),
        cfg.json.as_deref(),
        cfg.csv.as_deref(),
        cfg.json_nested,
    )?;
    run_loop(
        &cfg,
        list_devices,
        open_serial,
        Arc::new(Mutex::new(outputs)),
        stop,
        thread::sleep,
    );
    Ok(())
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod lib_tests;

#[cfg(test)]
#[path = "capture_tests.rs"]
mod capture_tests;
