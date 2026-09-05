# serial-capture

Dump all output from USB serial devices. Capture resumes when a device is reset or unplugged and replugged. New USB serial devices are picked up without restarting the tool.

Logs can be written as newline-delimited text, newline-delimited JSON, and/or CSV. Multiple formats can be enabled at once.

## Install

```bash
cargo install --path .
```

Or run from this repo:

```bash
cargo run -- --list
```

## Usage

Auto-discover USB serial devices (`/dev/ttyUSB*`, `/dev/ttyACM*`, and similar) and print text logs to stdout:

```bash
serial-capture
```

Capture a specific device and write every format:

```bash
serial-capture \
  --device /dev/ttyUSB0 \
  --baud 115200 \
  --text capture.txt \
  --json capture.json \
  --csv capture.csv
```

List devices and exit:

```bash
serial-capture --list
```

`--device` can be repeated. If omitted, every USB serial port is captured and ports that appear later are added automatically. After a USB reset, the same device is rematched by USB identity (VID/PID/serial) even if its `/dev/ttyUSB*` path changes.

Use `-` as a path to write a format to stdout. Log files are opened in append mode so reconnects do not truncate previous output.

On Linux, the capturing user typically needs membership in the `dialout` group.

## Docker

Docker is optional for local development. CI builds and publishes the image to GHCR.

```bash
docker compose build
docker compose run --rm --privileged app --list
```

Host `/dev` is mounted so USB serial devices are visible inside the container.

## Development

```bash
make test
make lint
make coverage
```

Coverage requires `cargo-llvm-cov`.
