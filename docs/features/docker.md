# Docker

Optional container build for local runs. CI publishes images to GHCR.

## Overview

`Dockerfile` is a two-stage build: `rust:1.88-bookworm` compiles a release binary, `debian:bookworm-slim` runs it. The image entrypoint is `serial-capture`. Extra compose arguments are passed through to the CLI.

Host `/dev` must be visible inside the container for USB serial devices.

## Compose

`compose.yaml` defines:

| Service | Purpose |
|---------|---------|
| `app` | Privileged run with `/dev` mounted and `./logs` at `/logs`. Default command: `--text /logs/capture.txt`. |
| `test` | Build stage image, `cargo test --all-targets` with the repo bind-mounted. |

```bash
docker compose build
docker compose run --rm --privileged app --list
docker compose run --rm --privileged app --device /dev/ttyUSB0 --text /logs/capture.txt
```

`--privileged` is required for the same USB access as the `app` service.

`.env.example` lists optional compose overrides (`BAUD`, `DEVICE`). The binary reads CLI flags, not those environment variables. Pass flags on the `compose run` command line.

## Published image

CI (`.github/workflows/container.yml`) builds `linux/amd64` and `linux/arm64` and pushes to `ghcr.io/<owner>/<repo>` on pushes to `main` and tags `v*`. Pull requests build but do not push.

## Related

- [Getting started](../getting-started.md)
- [CLI](cli.md)
