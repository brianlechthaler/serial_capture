# Reconnect and identity

Keep capturing after a USB reset or unplug/replug, including when the `/dev/ttyUSB*` path changes. In auto mode, ports that appear later get their own capture thread.

## Overview

Each selected target gets one thread. The thread opens the current path from a shared `Registry`, reads until the port closes or errors, then retries after `--poll-ms`. Incomplete lines are flushed on EOF, I/O error, or stop.

Identity is:

| USB info | Key |
|----------|-----|
| VID/PID and non-empty serial | `vvvv:pppp:<serial>` |
| VID/PID, no serial | `vvvv:pppp:<path>` |
| No USB info | device path |

`vvvv` and `pppp` are 4-digit lowercase hex.

## Auto mode (no `--device`)

Every discovered USB serial device is a target (CLI `--all`). The thread key is `device.id()`, so the same USB identity keeps one thread even if the tty path changes. A new identity (another adapter plugged in) starts a new thread, up to 32.

## Explicit `--device`

Targets are the requested paths. The loop still runs for missing paths: open fails, the thread sleeps `poll_ms`, and tries again.

After the requested path is seen with USB info, later appearances of that same identity on a different path update the registry. The original `--device` argument still names the target, but `resolve` returns the current tty path.

```mermaid
sequenceDiagram
  participant Loop as run_loop
  participant Reg as Registry
  participant Th as capture thread
  Loop->>Reg: bind /dev/ttyUSB0 to 0001:0002:SN
  Th->>Reg: resolve /dev/ttyUSB0
  Reg-->>Th: /dev/ttyUSB0
  Note over Loop: device resets, now ttyUSB1
  Loop->>Reg: bind same SN at /dev/ttyUSB1
  Th->>Reg: resolve /dev/ttyUSB0
  Reg-->>Th: /dev/ttyUSB1
```

## Capture thread behavior

1. Resolve the current path for the target key.
2. Open at `--baud` with a 100 ms read timeout. RTS is set before DTR. Both are deasserted unless `--dtr` is set. Clearing only DTR would reset ESP32 USB-JTAG.
3. Split on `\n`, strip a trailing `\r`, decode as UTF-8 lossy. Empty lines are dropped. Complete JSON objects or arrays glued on one line become separate records; truncated fragments and non-JSON text stay as-is.
4. On timeout, keep reading. On EOF or other error, flush a partial line and retry.
5. On stop, flush a partial line and exit the thread.

Open failures (device not present yet, permission denied) wait `poll_ms` and retry. They are not printed.

## Related

- [Device discovery](device-discovery.md)
- [Architecture](../architecture.md)
- [Log formats](log-formats.md)
