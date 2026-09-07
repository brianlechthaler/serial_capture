# Docker

Optional container build for local runs. CI publishes images to GHCR.

## Overview

`Dockerfile` is a two-stage build: `rust:1.88-bookworm` (digest-pinned) compiles a release binary, `debian:bookworm-slim` (digest-pinned) runs it as user `capture` (uid 65532, group `dialout`). The image entrypoint is `serial-capture`. Extra compose arguments are passed through to the CLI.

Host `/dev` must be visible inside the container for USB serial devices. Compose does **not** use `--privileged`. It drops capabilities, sets `no-new-privileges`, allows USB serial cgroup majors `166` (ttyACM) and `188` (ttyUSB), and bind-mounts `/dev` plus read-only `/sys`.

## Compose

`compose.yaml` defines:

| Service | Purpose |
|---------|---------|
| `app` | Non-root run with USB cgroup rules, `/dev` and `/sys` mounted, `./logs` at `/logs`. Default command: `--all --text /logs/capture.txt`. Memory 256m, 64 PIDs. |
| `test` | Build stage image as uid 1000, `cargo test --all-targets` with the repo bind-mounted. |

```bash
docker compose build
docker compose run --rm app --list
docker compose run --rm app --device /dev/ttyUSB0 --text /logs/capture.txt
```

`.env.example` lists optional compose overrides (`BAUD`, `DEVICE`). The binary reads CLI flags, not those environment variables. Pass flags on the `compose run` command line.

## Published image

CI (`.github/workflows/container.yml`) builds `linux/amd64` and `linux/arm64` and pushes to `ghcr.io/<owner>/<repo>` on pushes to `main` and tags `v*`. Pull requests build but do not push.

## Related

- [Getting started](../getting-started.md)
- [CLI](cli.md)
