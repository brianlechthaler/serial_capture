# serial-capture

CLI that dumps output from USB serial devices. Capture continues after a reset or unplug/replug. New USB serial ports are added without restarting.

## Quick start

```bash
serial-capture --list
serial-capture --all
```

On Linux, the capturing user typically needs membership in the `dialout` group. See [Getting started](docs/getting-started.md).

## Documentation

- [Getting started](docs/getting-started.md)
- [Architecture](docs/architecture.md)
- [Security audit](docs/security-audit.md)
- [CLI](docs/features/cli.md)
- [Device discovery](docs/features/device-discovery.md)
- [Reconnect and identity](docs/features/reconnect.md)
- [Log formats](docs/features/log-formats.md)
- [Docker](docs/features/docker.md)

## Requirements

- Rust 1.88+ (edition 2021)
- USB serial device visible as `/dev/ttyUSB*`, `/dev/ttyACM*`, or an equivalent name on macOS/Windows

## License

MIT
