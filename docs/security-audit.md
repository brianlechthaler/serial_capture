# Security audit — serial-capture

**Date:** 2026-09-06
**Scope:** full repository (not a diff-only review)
**Mode:** findings below were patched in this branch; Trivy Dockerfile misconfig count is now 0

This is a local Rust CLI that opens USB serial ports and appends captured lines to text/JSON/CSV files or stdout. There is no HTTP server, no authentication, and no MCP surface.

## Remediation status

All 15 findings were fixed on this branch:

| ID | Was | Fix |
|----|-----|-----|
| SEC-001 | High | Compose: no `privileged`; `cap_drop: ALL`, `no-new-privileges`, USB cgroup majors 166/188 |
| SEC-002 | High | Dockerfile `USER capture` (uid 65532, group `dialout`) |
| SEC-003 | Medium | `LineSplitter` caps at 1 MiB |
| SEC-004 | Medium | CSV formula prefixes get a leading `'` |
| SEC-005 | Medium | Actions pinned to commit SHAs; CI toolchain `1.88.0` |
| SEC-006 | Medium | Capture requires `--device` or `--all` |
| SEC-007 | Medium | Base images pinned by digest |
| SEC-008 | Low | Unix log files `0600` |
| SEC-009 | Low | `.gitignore` includes `.env` |
| SEC-010 | Low | Poll floor 50 ms; max 32 capture threads |
| SEC-011 | Low | JSON `data` is a string unless `--json-nested` |
| SEC-012 | Low | Compose mem/PID limits; test service uid 1000 |
| SEC-013 | Info | `HEALTHCHECK CMD kill -0 1` |
| SEC-014 | Info | `audit.yml` (`cargo audit`) + Dependabot |
| SEC-015 | Info | Mutex poison recovered with `into_inner()` |

**Re-scan:** Trivy config on `Dockerfile` — 0 misconfigurations (was DS-0002 High, DS-0026 Low). Tests 63, coverage 100% functions / 99.60% lines.

Residual risk: the operator can still point `--text` / `--device` at any path they can open. That is the CLI trust model.

The original finding write-ups follow.

## Executive summary (as found)

**15 findings (0 critical, 2 high, 5 medium, 5 low, 3 informational).**


No production secrets, no known crate CVEs, and no remote unauthenticated RCE. The two high issues are on the Docker path: Compose runs the app `--privileged` with host `/dev` mounted, and the image has no `USER` so the process is root. Together that is a host-compromise recipe for anyone who starts the stack.

Top 3 risks:

1. **SEC-001** — `compose.yaml` uses `privileged: true` and bind-mounts `/dev`.
2. **SEC-002** — Runtime image runs as root (Trivy DS-0002).
3. **SEC-003** — `LineSplitter` grows without a cap; a device that never sends a newline can exhaust memory.

## Attack surface

| Area | What exists |
|------|-------------|
| Languages | Rust 2021 (`serial-capture` 0.1.0) |
| Entry points | CLI (`Config` / clap), optional Docker/Compose, GitHub Actions (test, lint, container/GHCR) |
| Auth | None (local process, operator identity) |
| Data | USB serial payloads, USB VID/PID/serial strings, log files |
| Trust boundary | Operator argv → process → `/dev` tty + log paths; USB device → capture threads → logs |
| Out of scope as absent | HTTP, WebSockets, cookies, CORS, SSRF, SQL, MCP, Terraform, Kubernetes |

## Scanner results

| Scanner | Result |
|---------|--------|
| gitleaks (HEAD + 7 commits of history) | No leaks |
| Trivy fs (`vuln,secret,misconfig`) | Cargo.lock: 0 vulns; Dockerfile: DS-0002 High, DS-0026 Low |
| OSV.dev query of 81 `Cargo.lock` crates | 0 advisories |
| `cargo-audit` | Not run to completion (install hung on this host); OSV + Trivy cover the same advisory set |
| Semgrep | Not installed |

## Findings

### SEC-001 — Privileged Compose with host `/dev`

| | |
|--|--|
| **Severity** | High |
| **Category** | Config / containers |
| **Location** | `compose.yaml:3-6` |

**Evidence:** The `app` service sets `privileged: true` and mounts `/dev:/dev`. Docs tell operators to pass `--privileged` on `docker compose run` as well (`docs/features/docker.md`).

**Impact:** A container started from this file has host device access and almost all Linux capabilities. Combined with root (SEC-002) this is a straightforward host compromise if anything in the container is attacker-controlled (malicious image rebuild, bind-mounted workspace, USB gadget fuzzing the binary as root).

**Remediation:** Drop `privileged`. Prefer `device_cgroup_rules` for USB major numbers (`188` ttyUSB, `166` ttyACM) plus a narrower `/dev` bind, or pass specific `--device` mappings. Document that full privileged mode is not required for listing or capturing a known tty.

### SEC-002 — Container process runs as root

| | |
|--|--|
| **Severity** | High |
| **Category** | Config / containers |
| **Location** | `Dockerfile` (no `USER`); Trivy `DS-0002` |

**Evidence:** Final stage is `FROM debian:bookworm-slim` copying the binary to `/usr/local/bin/serial-capture`. No `useradd` / `USER`. Trivy: “Specify at least 1 USER command in Dockerfile with non-root user as argument.”

**Impact:** The capture process can read/write any path the container can see (including the Compose `./logs` mount and, with SEC-001, host devices). A bug in serial parsing or a malicious log path then runs with uid 0.

**Remediation:** Create a dedicated user, add it to `dialout` (GID should match the host if device nodes are bind-mounted), and `USER` that account. Keep the binary mode `0755` owned by root.

### SEC-003 — Unbounded line buffer

| | |
|--|--|
| **Severity** | High → recorded as **Medium** (local/USB attacker, no remote socket) |
| **Category** | Denial of service |
| **Location** | `src/capture.rs:19-21` (`LineSplitter::push`) |

**Evidence:** `push` extends `self.buf` with every read and only drains on `\n`. There is no max length. Reads are 4096 bytes at a time with a 100 ms timeout, so a device that streams without newlines grows the `Vec` without bound.

**Impact:** Memory exhaustion of the capture process (and the host, if Compose is privileged/root). A USB gadget or buggy firmware is enough.

**Remediation:** Cap the splitter (for example 1 MiB). On overflow, emit a truncated line or drop the chunk and log an error to stderr.

### SEC-004 — CSV formula injection from device data

| | |
|--|--|
| **Severity** | Medium |
| **Category** | Injection |
| **Location** | `src/output.rs:37-44` (`format_csv`) |

**Evidence:** CSV escaping quotes commas and newlines (`csv_escape`) but does not neutralize leading `=`, `+`, `-`, or `@`. Device bytes become the `data` column after UTF-8 lossy decode.

**Impact:** Opening a capture CSV in Excel/LibreOffice can execute formulas. A USB device can plant `=cmd|' /C calc'!A0`-style payloads in logs.

**Remediation:** Prefix formula-like fields with a single quote, or force every `data` field to be quoted and strip leading formula characters. Document that CSVs are untrusted device output.

### SEC-005 — GitHub Actions pinned to mutable tags

| | |
|--|--|
| **Severity** | Medium |
| **Category** | Supply chain / CI |
| **Location** | `.github/workflows/{test,lint,container}.yml` |

**Evidence:** Third-party actions use floating tags, including `dtolnay/rust-toolchain@stable` (moves whenever stable Rust changes), `actions/checkout@v4`, `Swatinem/rust-cache@v2`, `taiki-e/install-action@v2`, and `docker/*@v3`–`@v6`. Workflow `permissions` are otherwise tight (`contents: read`; `packages: write` only on the container job). No `pull_request_target`.

**Impact:** A compromised or retagged action can execute in CI with `GITHUB_TOKEN` (GHCR push on `main` / tags). `@stable` is worse than `@v4` because it is not even a major-version pin.

**Remediation:** Pin every action to a commit SHA. Replace `@stable` with a toolchain file or an explicit version. Optionally add `cargo audit` / Dependabot.

### SEC-006 — Default capture of every USB serial device

| | |
|--|--|
| **Severity** | Medium |
| **Category** | Data exposure |
| **Location** | `src/lib.rs:20-38`, `src/device.rs:251-262` (`Selector::Auto`) |

**Evidence:** With no `--device`, `run` captures every discovered USB tty and, if no log flags are set, writes text to stdout. Discovery matches `ttyUSB*`, `ttyACM*`, macOS `cu.usb*`, and Windows `COM\d+`.

**Impact:** A second adapter, LTE modem, or security device with a CDC interface is silently logged. Payloads can include AT commands, bootloader secrets, or PII. Auto mode also starts an unbounded number of threads (see SEC-010).

**Remediation:** Default to `--list` or require `--device` / `--all`. If auto mode stays, print the selected identities to stderr at start and refuse to run when more than N devices appear unless `--all` is set.

### SEC-007 — Container base images not pinned by digest

| | |
|--|--|
| **Severity** | Medium |
| **Category** | Supply chain |
| **Location** | `Dockerfile:3`, `Dockerfile:13` |

**Evidence:** `FROM rust:1.88-bookworm` and `FROM debian:bookworm-slim` with no `@sha256:…`. Rebuilds float to whatever those tags currently resolve to.

**Impact:** A registry-tag move or a compromised rebuild of `bookworm-slim` is picked up on the next CI image build and published to GHCR.

**Remediation:** Pin both stages by digest. Refresh pins on a schedule.

### SEC-008 — Capture logs created with default umask

| | |
|--|--|
| **Severity** | Low |
| **Category** | Data protection |
| **Location** | `src/output.rs:60-67` (`OpenOptions::new().create(true).append(true)`) |

**Evidence:** Files are created without `0o600`. Typical umask `022` yields `0644`. Serial lines often contain credentials from device consoles.

**Impact:** Any local user who can read the log path can recover captured secrets.

**Remediation:** `OpenOptionsExt::mode(0o600)` on Unix. Document directory permissions for `./logs`.

### SEC-009 — `.env` is not gitignored

| | |
|--|--|
| **Severity** | Low |
| **Category** | Secrets / gitignore |
| **Location** | `.gitignore` |

**Evidence:** `.env.example` is tracked (comments only). `.gitignore` covers `/target`, `logs/`, and `*.log`, but not `.env`. gitleaks found no live secrets in history.

**Impact:** A future local override copied from `.env.example` can be committed accidentally. Today the CLI does not read env vars (`.env.example` says so).

**Remediation:** Add `.env` to `.gitignore`. Keep `.env.example`.

### SEC-010 — Tight poll loop and unbounded capture threads

| | |
|--|--|
| **Severity** | Low |
| **Category** | Denial of service |
| **Location** | `src/config.rs:61-63`, `src/capture.rs:122-157` |

**Evidence:** `--poll-ms 0` becomes 1 ms. Auto mode spawns one thread per USB identity and never retires the key from `spawned`.

**Impact:** CPU spin and thread growth if many gadgets enumerate. Local only.

**Remediation:** Floor `poll_ms` at 50–100 ms unless overridden. Bound concurrent capture threads. Reap identities that disappear after a timeout.

### SEC-011 — Device JSON nested into JSONL `data`

| | |
|--|--|
| **Severity** | Low |
| **Category** | Injection / log consumers |
| **Location** | `src/output.rs:26-35` (`format_json`) |

**Evidence:** If a line parses as JSON, it is embedded as a structured `data` value rather than a string. Envelope keys `ts` / `device` are not overwritten.

**Impact:** Downstream log shippers that flatten objects can grow attacker-controlled keys, types, or large documents. `serde_json` recursion limits prevent stack overflow here (parse failure falls back to a string).

**Remediation:** Keep `data` as a string by default; add `--json-nested` for the current behavior. Cap parsed value size.

### SEC-012 — Compose test service and missing runtime limits

| | |
|--|--|
| **Severity** | Low |
| **Category** | Config / containers |
| **Location** | `compose.yaml` `test` service; `app` has no `mem_limit` / `pids_limit` / `security_opt` |

**Evidence:** `test` bind-mounts the repo at `/workspace` and runs `cargo test` as root in the build stage. `app` has no cgroup limits.

**Impact:** A test or capture runaway can fill host disk/CPU. The test mount lets a container-root process write the host tree.

**Remediation:** Run tests as a non-root user; add `mem_limit`, `pids_limit`, and `security_opt: ["no-new-privileges:true"]` on `app`.

### SEC-013 — No Docker HEALTHCHECK

| | |
|--|--|
| **Severity** | Informational |
| **Category** | Config |
| **Location** | `Dockerfile`; Trivy `DS-0026` |

The process is a blocking CLI, not a server. A HEALTHCHECK that execs the binary would start a second capture. Skip unless an orchestrator requires a no-op check (`CMD kill -0 1`).

### SEC-014 — No dependency advisory job or Dependabot

| | |
|--|--|
| **Severity** | Informational |
| **Category** | Supply chain |
| **Location** | `.github/workflows/` |

Lockfile is committed and CI uses `cargo build --release --locked`. There is no `cargo audit` / `cargo deny` workflow and no Dependabot config. Current OSV/Trivy crate scans are clean.

**Remediation:** Add a `cargo audit` job and Dependabot for `cargo` and `github-actions`.

### SEC-015 — Mutex poison panics the poll loop

| | |
|--|--|
| **Severity** | Informational |
| **Category** | Availability |
| **Location** | `src/capture.rs:125`, `src/capture.rs:139` |

`registry.lock().unwrap()` panics if another thread poisoned the mutex. `emit_lines` already survives poison. Not attacker-interesting beyond turning a bug into a hard abort.

**Remediation:** Use `unwrap_or_else(|e| e.into_inner())` like `emit_lines`.

## Checklist (phases 1–8)

| Phase | Outcome |
|-------|---------|
| 1 Secrets | Clean (gitleaks + Trivy secret). `.env` gitignore gap = SEC-009 |
| 2 Dependencies | 0 CVEs (OSV + Trivy). Unpinned actions/images = SEC-005, SEC-007, SEC-014 |
| 3 App code | No command/SQL injection. CSV formula + unbounded buffer + JSON nest = SEC-003, SEC-004, SEC-011. Auth N/A |
| 4 API/web | N/A (no HTTP) |
| 5 Infra/CI | Privileged Compose, root image, unpinned actions = SEC-001, SEC-002, SEC-005, SEC-012. No `pull_request_target` |
| 6 Data/privacy | Logs may hold device secrets; default umask = SEC-008; auto-capture = SEC-006 |
| 7 MCP | N/A (no MCP server) |
| 8 Hardening | HEALTHCHECK N/A-ish = SEC-013; no rate limits (CLI) |

## Residual risk if all High/Medium items are fixed

The operator can still point `--text` / `--device` at any path they can open. That is the intended CLI trust model and is not treated as a vulnerability.

```
Total: 15 findings (0 critical, 2 high, 5 medium, 5 low, 3 informational)
Status: all remediations applied on this branch (Trivy Dockerfile: 0 misconfigs)
```
