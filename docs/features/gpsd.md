# GPSD

Optional lat/lon columns from a local [gpsd](https://gpsd.gitlab.io/gpsd/) daemon.

## Overview

`--gpsd` starts a background client on `--gpsd-addr` (default `127.0.0.1:2947`). Each log line is annotated with the latest TPV fix. Capture still runs if gpsd is down or has no fix; those rows get empty (text/CSV) or `null` (JSON) coordinates.

Without `--gpsd`, log shape is unchanged: no `lat`/`lon` fields.

Timestamps still come from the host clock. GPS time is not used here.

## Usage

```bash
serial-capture --all --gpsd
serial-capture --device /dev/ttyUSB0 --gpsd --gpsd-addr 127.0.0.1:2947 \
  --text capture.txt --json capture.json --csv capture.csv
```

gpsd must already be running and providing a TPV fix. This tool does not open a GPS receiver itself.

From Docker, gpsd on the host is not `127.0.0.1` inside the container. Use host networking or the host gateway, for example `--gpsd-addr 172.17.0.1:2947`. gpsd must listen on an address the container can reach.

## Log columns

When `--gpsd` is on:

| Format | Shape |
|--------|-------|
| Text | `ts`, device, lat, lon, data (tab-separated) |
| JSON | `lat` and `lon` numbers, or `null` |
| CSV | header `ts,device,lat,lon,data` |

```text
2023-11-14T22:13:20.123Z	/dev/ttyUSB0	37.5	-122.25	hello
```

```json
{"data":"hello","device":"/dev/ttyUSB0","lat":37.5,"lon":-122.25,"ts":"2023-11-14T22:13:20.123Z"}
```

```csv
ts,device,lat,lon,data
2023-11-14T22:13:20.123Z,/dev/ttyUSB0,37.5,-122.25,hello
```

Negative longitudes are written as numbers in CSV (no leading `'`). Missing fixes are empty cells.

## Configuration

| Option | Default | Description |
|--------|---------|-------------|
| `--gpsd` | off | Connect to gpsd and add lat/lon columns. |
| `--gpsd-addr` | `127.0.0.1:2947` | gpsd TCP `host:port`. |

The client sends `?WATCH={"enable":true,"json":true}` and keeps the last TPV that includes both `lat` and `lon`. Disconnects retry every `--poll-ms` (minimum 50 ms).

## Troubleshooting

| Symptom | Cause |
|---------|--------|
| Columns present but empty/`null` | No TPV yet, gpsd not running, or TPV without lat/lon (no fix). |
| Connection retries, no coords | `--gpsd-addr` unreachable; default is loopback TCP 2947. |
| Docker never gets a fix | Container `127.0.0.1` is not the host gpsd. |

## Related

- [CLI](cli.md)
- [Log formats](log-formats.md)
- [Docker](docker.md)
