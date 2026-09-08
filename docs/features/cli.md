# CLI

Command-line flags for `serial-capture`.

## Overview

The binary is a thin wrapper around `serial_capture::run(Config::parse())`. Parse errors and runtime errors are printed to stderr; the process exits `1` on failure.

`--help` and `--version` are provided by clap.

## Usage

```bash
serial-capture [OPTIONS]
```

## Configuration

| Option | Default | Description |
|--------|---------|-------------|
| `-d`, `--device <PATH>` | (none) | Repeatable. Required unless `--all` or `--list`. |
| `--all` | off | Capture every USB serial port. |
| `-b`, `--baud <RATE>` | `115200` | Serial baud rate. |
| `--text <PATH>` | `-` if no format is set | Newline-delimited text log. `-` is stdout. |
| `--json <PATH>` | unset | Newline-delimited JSON log. `-` is stdout. |
| `--json-nested` | off | Parse serial lines as JSON values inside `--json` `data`. Default keeps `data` as a string. |
| `--csv <PATH>` | unset | CSV log. `-` is stdout. Formula-like fields (`=`, `+`, `-`, `@`) are prefixed with `'`. |
| `--gpsd` | off | Add lat/lon columns from gpsd. See [GPSD](gpsd.md). |
| `--gpsd-addr` | `127.0.0.1:2947` | gpsd TCP address. Used only with `--gpsd`. |
| `--poll-ms <MS>` | `500` | Device scan and reconnect retry interval. Values below 50 are treated as 50. |
| `--list` | off | Print USB serial devices and exit. Does not open ports or logs. |

If `--list` is off, pass `--device` (repeatable) or `--all`. Omitting both is an error. `--all` captures every matching USB serial port, including ones that appear later, up to 32 concurrent devices.

Log files are opened with create + append and Unix mode `0600`. Parent directories must already exist; a missing directory fails before the capture loop starts.

## Examples

List devices:

```bash
serial-capture --list
```

Stdout text for every USB serial port:

```bash
serial-capture --all
```

Two explicit devices, JSON only, faster rescan:

```bash
serial-capture -d /dev/ttyUSB0 -d /dev/ttyACM0 --json - --poll-ms 200
```

Text to stdout with GPS coordinates:

```bash
serial-capture --all --gpsd
```

## Related

- [Getting started](../getting-started.md)
- [Log formats](log-formats.md)
- [GPSD](gpsd.md)
- [Device discovery](device-discovery.md)
