# Getting started

Install, list devices, and start capturing USB serial output.

## Install

From this repo:

```bash
cargo install --path .
```

Or run without installing:

```bash
cargo run -- --list
```

The binary name is `serial-capture`.

## Permissions (Linux)

Opening a tty usually requires membership in the `dialout` group:

```bash
sudo usermod -aG dialout "$USER"
```

Log out and back in (or reboot) so the new group takes effect.

If `serial-capture --list` shows devices but capture fails with a permission error, the process is not in `dialout`.

## First capture

List USB serial devices and exit:

```bash
serial-capture --list
```

Example line:

```text
/dev/ttyUSB0  2341:0043  serial=ABC  Uno
```

Capture every matching USB serial port and print newline-delimited text to stdout:

```bash
serial-capture --all
```

If `--text`, `--json`, and `--csv` are all omitted, the tool writes text to stdout (`-`).

Capture one device to files:

```bash
serial-capture \
  --device /dev/ttyUSB0 \
  --baud 115200 \
  --text capture.txt \
  --json capture.json \
  --csv capture.csv
```

`--device` can be repeated. Use `--all` (no `--device`) to capture every matching port, including ones that appear later.

ESP32 USB-JTAG (`/dev/ttyACM*`) resets when RTS is asserted and DTR is deasserted. `serial-capture` deasserts RTS then DTR on open. Pass `--dtr` only when the device should reset on open (typical Arduino auto-reset).

JSON event streams (one object per line) can use `--json - --json-nested`. Glued objects on one line are split; empty lines are dropped.

## Development

```bash
make test
make lint
make coverage
make mcp
```

Coverage requires `cargo-llvm-cov`. Thresholds are 100% functions and 99% lines.

## Related

- [CLI flags](features/cli.md)
- [GPSD](features/gpsd.md)
- [MCP](mcp.md)
- [Device discovery](features/device-discovery.md)
- [Docker](features/docker.md)
