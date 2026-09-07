# Device discovery

Find USB serial ports and print them with `--list`.

## Overview

Discovery merges two sources:

1. A scan of `/dev` for USB tty names, with USB descriptors read from `/sys/class/tty/<name>/device` (walk parents until `idVendor` exists).
2. `serialport::available_ports()`, filtered to the same tty name rules.

If both sources report the same path, existing USB info from the first source is kept. Results are sorted by path.

Non-USB names such as `ttyS0` are ignored.

## Recognized names

A path matches if its last component is one of:

| Pattern | Typical platform |
|---------|------------------|
| `ttyUSB*` | Linux USB-serial |
| `ttyACM*` | Linux CDC-ACM |
| `cu.usb*`, `tty.usb*` | macOS |
| `cu.wchusb*`, `tty.wchusb*` | macOS (WCH chips) |
| `COM` followed by digits | Windows |

Matching is case-insensitive for those prefixes. `COM` with no digits is not a match.

## `--list` output

Each line is `display_line()`:

```text
<path>  <vid>:<pid>  serial=<serial>  <product or manufacturer>
```

USB fields are hex `vid:pid`. `serial=` is omitted when empty. Product is preferred over manufacturer. Devices with no USB info print the path only.

```bash
serial-capture --list
```

`--list` does not start capture threads and does not apply the text-to-stdout default.

## Related

- [Reconnect and identity](reconnect.md)
- [CLI](cli.md)
