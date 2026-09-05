use clap::Parser;

#[derive(Parser, Debug, Clone, PartialEq, Eq)]
#[command(
    name = "serial-capture",
    about = "Dump USB serial device output with auto-reconnect",
    version
)]
pub struct Config {
    /// Serial device path (repeatable). Omit to auto-discover USB serial devices.
    #[arg(short, long)]
    pub device: Vec<String>,

    /// Baud rate
    #[arg(short, long, default_value_t = 115_200)]
    pub baud: u32,

    /// Newline-delimited text log (`-` for stdout)
    #[arg(long)]
    pub text: Option<String>,

    /// Newline-delimited JSON log (`-` for stdout)
    #[arg(long)]
    pub json: Option<String>,

    /// CSV log (`-` for stdout)
    #[arg(long)]
    pub csv: Option<String>,

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
            text: None,
            json: None,
            csv: None,
            poll_ms: 500,
            list: false,
        }
    }
}

impl Config {
    pub fn with_output_defaults(mut self) -> Self {
        if !self.list && self.text.is_none() && self.json.is_none() && self.csv.is_none() {
            self.text = Some("-".into());
        }
        self
    }

    pub fn poll_duration(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.poll_ms.max(1))
    }
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod config_tests;
