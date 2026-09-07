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
serial-capture
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

`--device` can be repeated. Omit it to auto-discover ports and pick up new ones as they appear.

## Development

```bash
make test
make lint
make coverage
```

Coverage requires `cargo-llvm-cov`. Thresholds are 100% functions and 99% lines.

## Related

- [CLI flags](features/cli.md)
- [Device discovery](features/device-discovery.md)
- [Docker](features/docker.md)
