# MCP server — serial-capture-logs

Read-only stdio MCP server for USB serial device lists and allowlisted capture logs. Implementation: `serial-capture-mcp` binary (`src/mcp.rs`). Project notes: [docs/mcp.md](../../docs/mcp.md).

## Tools (v1.0.0)

| Tool | Scope | Behavior |
|------|-------|----------|
| `list_devices` | `devices:list` | USB serial inventory (no port open) |
| `list_logs` | `logs:read` | Files in `SERIAL_CAPTURE_LOG_DIR` with `.txt` `.json` `.jsonl` `.csv` `.log` |
| `search_logs` | `logs:read` | Literal search, `limit` max 50 |
| `read_log` | `logs:read` | Last `max_lines` (max 200) of one file |
| `mcp_inventory` | `inventory:read` | Pinned tool fingerprints, stdio bind, sandbox flags |

No write tools. No shell, no capture start, no arbitrary paths.

## Env

See [.env.example](.env.example). Required for log tools: `SERIAL_CAPTURE_LOG_DIR`.

## Start

```bash
make mcp
# or
cargo run --bin serial-capture-mcp
```

Cursor `.cursor/mcp.json` launches that debug binary over stdio (`type: stdio`). Build it first (`cargo build --bin serial-capture-mcp`), then reload MCP. Existing chats do not pick up a newly attached server.

Sandboxed compose (no network, read-only root, logs mounted `:ro`):

```bash
docker compose run --rm -T mcp
```

## Capabilities

Read-only. Secrets in log text are redacted before the agent sees them. Audit JSONL is hash-chained (`MCP_AUDIT_PATH`).

## Security

NSA CSI PP-26-1834 mapping and threat model: [docs/mcp.md](../../docs/mcp.md).
