use clap::Parser;

#[derive(Parser, Debug, Clone, PartialEq, Eq)]
#[command(
    name = "serial-capture",
    about = "Dump USB serial device output with auto-reconnect",
    version
)]
pub struct Config {
    /// Serial device path (repeatable). Required unless --all or --list.
    #[arg(short, long)]
    pub device: Vec<String>,

    /// Baud rate
    #[arg(short, long, default_value_t = 115_200)]
    pub baud: u32,

    /// Capture every USB serial device. Required when --device is omitted.
    #[arg(long)]
    pub all: bool,

    /// Newline-delimited text log (`-` for stdout)
    #[arg(long)]
    pub text: Option<String>,

    /// Newline-delimited JSON log (`-` for stdout)
    #[arg(long)]
    pub json: Option<String>,

    /// CSV log (`-` for stdout)
    #[arg(long)]
    pub csv: Option<String>,

    /// Parse serial lines as JSON values in --json output
    #[arg(long)]
    pub json_nested: bool,

    /// Add lat/lon columns from gpsd
    #[arg(long)]
    pub gpsd: bool,

    /// gpsd host:port
    #[arg(long, default_value = "127.0.0.1:2947")]
    pub gpsd_addr: String,

    /// Use gpsd TPV time for log timestamps
    #[arg(long, requires = "gpsd")]
    pub gpsd_time: bool,

    /// How often to scan for devices, in milliseconds
    #[arg(long, default_value_t = 500)]
    pub poll_ms: u64,

    /// List USB serial devices and exit
    #[arg(long)]
    pub list: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            device: Vec::new(),
            baud: 115_200,
            all: false,
            text: None,
            json: None,
            csv: None,
            json_nested: false,
            gpsd: false,
            gpsd_addr: "127.0.0.1:2947".into(),
            gpsd_time: false,
            poll_ms: 500,
            list: false,
        }
    }
}

pub const MIN_POLL_MS: u64 = 50;

impl Config {
    pub fn with_output_defaults(mut self) -> Self {
        if !self.list && self.text.is_none() && self.json.is_none() && self.csv.is_none() {
            self.text = Some("-".into());
        }
        self
    }

    pub fn poll_duration(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.poll_ms.max(MIN_POLL_MS))
    }
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod config_tests;
