# Log formats

Write captured lines as text, JSON, and/or CSV. Any combination can be enabled at once.

## Overview

Each complete line becomes a `Record`: UTC timestamp (RFC 3339 with milliseconds), device path at emit time, and line text. Destinations are opened once at startup. Files use create + append so reconnects do not truncate previous output.

With `--gpsd`, text/JSON/CSV also include `lat` and `lon` from the latest gpsd TPV. `--gpsd-time` writes TPV `time` into `ts` when present. See [GPSD](gpsd.md).

Use `-` as a path to write that format to stdout. Multiple formats to stdout interleave on the same stream.

If no format flag is set (and `--list` is off), text to stdout is implied.

## Text

Tab-separated: timestamp, device, data.

```text
2023-11-14T22:13:20.123Z	/dev/ttyUSB0	hello
```

## JSON

One JSON object per line:

```json
{"data":"hello","device":"/dev/ttyUSB0","ts":"2023-11-14T22:13:20.123Z"}
```

If the line is valid JSON **and** `--json-nested` is set, `data` is that value (object, array, number, and so on). Otherwise `data` is a JSON string.

Empty and whitespace-only lines are dropped. If one newline-delimited line contains more than one JSON object or array, each complete value becomes its own record. A truncated fragment before or after those values is kept as a string. Espressif panic dumps and other non-JSON text stay strings.

```json
{"data":{"event":"config","beep_mask":31},"device":"/dev/ttyUSB0","ts":"2023-11-14T22:13:20.123Z"}
```

## CSV

Header `ts,device,data` is written when the destination is stdout, the file does not exist, or the file is empty. An existing non-empty file is appended without a second header.

Fields are RFC 4180-style escaped when they contain `"`, `,`, `\n`, or `\r` (quotes doubled, field wrapped in `"`). Values that start with `=`, `+`, `-`, `@`, tab, or CR get a leading `'` so spreadsheets do not treat them as formulas.

```csv
ts,device,data
2023-11-14T22:13:20.123Z,/dev/ttyUSB0,hello
```

## Usage

```bash
serial-capture --text capture.txt --json capture.json --csv capture.csv
serial-capture --json -
serial-capture --json - --json-nested
```

Each emit flushes the writer so lines show up immediately.

## Troubleshooting

| Symptom | Cause |
|---------|--------|
| Process exits immediately with a path error | Parent directory of a log file does not exist. |
| CSV has two header rows | Unlikely unless the previous file ended empty or was truncated to zero. Header is written only for empty/new files and stdout. |
| JSON `data` is a string, not an object | The serial line was not valid JSON, or `--json-nested` was omitted. Truncated objects and panic dumps stay strings. |

## Related

- [CLI](cli.md)
- [GPSD](gpsd.md)
- [Reconnect and identity](reconnect.md)
