use clap::Parser;
use serial_capture::Config;

fn main() {
    if let Err(err) = serial_capture::run(Config::parse()) {
        eprintln!("{err}");
        std::process::exit(1);
    }
}
