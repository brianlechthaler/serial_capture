use crate::device::{list_devices, Device};
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

type HmacSha256 = Hmac<Sha256>;

pub const SERVER_NAME: &str = "serial-capture-logs";
pub const SERVER_VERSION: &str = "1.0.0";
pub const PROTOCOL_VERSION: &str = "2024-11-05";
pub const MAX_LINE_BYTES: usize = 65_536;
pub const MAX_RESPONSE_BYTES: usize = 65_536;
#[cfg(not(test))]
pub const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
#[cfg(test)]
pub const MAX_FILE_BYTES: u64 = 1024;
const KEY_ID: &str = "mcp-hmac-1";
const ALLOWED_EXT: &[&str] = &["txt", "json", "jsonl", "csv", "log"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpError {
    Denied(&'static str),
    Message(String),
}

impl McpError {
    fn as_str(&self) -> String {
        match self {
            Self::Denied(s) => (*s).into(),
            Self::Message(s) => s.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Envelope {
    pub payload: String,
    pub signature: String,
    pub key_id: String,
    pub nonce: String,
    pub expires_at: String,
}

#[derive(Default)]
pub struct ReplayCache {
    seen: HashSet<String>,
}

#[derive(Clone)]
pub struct RateLimit {
    pub max: usize,
    pub window: Duration,
    hits: VecDeque<Instant>,
}

impl RateLimit {
    fn new(max: usize) -> Self {
        Self {
            max,
            window: Duration::from_secs(10),
            hits: VecDeque::new(),
        }
    }

    fn allow(&mut self) -> bool {
        let now = Instant::now();
        while self
            .hits
            .front()
            .is_some_and(|t| now.duration_since(*t) > self.window)
        {
            self.hits.pop_front();
        }
        if self.hits.len() >= self.max {
            return false;
        }
        self.hits.push_back(now);
        true
    }
}

pub struct McpState {
    pub log_dir: PathBuf,
    pub scopes: HashSet<String>,
    pub require_scopes: bool,
    pub hmac_key: Option<Vec<u8>>,
    pub require_signing: bool,
    pub caller: String,
    pub audit_path: Option<PathBuf>,
    pub gateway_token: Option<String>,
    pub caller_token: Option<String>,
    pub list_devices: fn() -> Vec<Device>,
    pub rate: RateLimit,
    replay: ReplayCache,
    prev_hash: String,
}

impl McpState {
    pub fn from_env() -> Self {
        Self::from_map(&std::env::vars().collect())
    }

    pub fn from_map(vars: &BTreeMap<String, String>) -> Self {
        let scopes = match vars.get("MCP_SCOPES") {
            Some(s) if !s.is_empty() => s.split(',').map(|p| p.trim().to_string()).collect(),
            _ => HashSet::from([
                "devices:list".into(),
                "logs:read".into(),
                "inventory:read".into(),
            ]),
        };
        Self {
            log_dir: vars
                .get("SERIAL_CAPTURE_LOG_DIR")
                .map(PathBuf::from)
                .unwrap_or_default(),
            scopes,
            require_scopes: vars.get("MCP_REQUIRE_SCOPES").map(String::as_str) == Some("1"),
            hmac_key: vars
                .get("MCP_HMAC_KEY")
                .filter(|s| !s.is_empty())
                .map(|s| parse_hmac_key(s)),
            require_signing: vars.get("MCP_REQUIRE_SIGNING").map(String::as_str) == Some("1"),
            caller: vars
                .get("MCP_CALLER_IDENTITY")
                .cloned()
                .unwrap_or_else(|| "agent:cursor-host".into()),
            audit_path: vars.get("MCP_AUDIT_PATH").map(PathBuf::from),
            gateway_token: vars.get("MCP_GATEWAY_TOKEN").cloned(),
            caller_token: vars.get("MCP_CALLER_TOKEN").cloned(),
            list_devices,
            rate: RateLimit::new(30),
            replay: ReplayCache::default(),
            prev_hash: "0".repeat(64),
        }
    }

    #[cfg(test)]
    pub fn for_test(log_dir: PathBuf) -> Self {
        let audit_path = Some(log_dir.with_extension("audit.jsonl"));
        Self {
            log_dir,
            scopes: HashSet::from([
                "devices:list".into(),
                "logs:read".into(),
                "inventory:read".into(),
            ]),
            require_scopes: false,
            hmac_key: None,
            require_signing: false,
            caller: "agent:test".into(),
            audit_path,
            gateway_token: None,
            caller_token: None,
            list_devices: || Vec::new(),
            rate: RateLimit::new(1000),
            replay: ReplayCache::default(),
            prev_hash: "0".repeat(64),
        }
    }
}

pub fn serve(stdin: impl BufRead, mut stdout: impl Write, state: &mut McpState) -> io::Result<()> {
    for line in stdin.lines() {
        let line = line?;
        if let Some(resp) = handle_line(state, &line) {
            writeln!(stdout, "{resp}")?;
            stdout.flush()?;
        }
    }
    Ok(())
}

pub fn handle_line(state: &mut McpState, line: &str) -> Option<String> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    if line.len() > MAX_LINE_BYTES {
        return Some(rpc_error(Value::Null, -32600, "request too large"));
    }
    let msg: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => return Some(rpc_error(Value::Null, -32700, "parse error")),
    };
    let method = msg.get("method").and_then(Value::as_str);
    let id = msg.get("id").cloned();
    if method.is_some_and(|m| m.starts_with("notifications/")) && id.is_none() {
        return None;
    }
    let id = id.unwrap_or(Value::Null);
    if msg.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Some(rpc_error(id, -32600, "jsonrpc must be 2.0"));
    }
    let params = msg.get("params").cloned().unwrap_or(json!({}));
    let result = match method {
        Some("initialize") => json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {"tools": {"listChanged": false}},
            "serverInfo": {"name": SERVER_NAME, "version": SERVER_VERSION}
        }),
        Some("ping") => json!({}),
        Some("tools/list") => json!({"tools": tool_defs()}),
        Some("tools/call") => return Some(rpc_ok(id, call_tool(state, &params))),
        Some(_) => return Some(rpc_error(id, -32601, "method not found")),
        None => return Some(rpc_error(id, -32600, "missing method")),
    };
    Some(rpc_ok(id, result))
}

fn call_tool(state: &mut McpState, params: &Value) -> Value {
    let name = match params.get("name").and_then(Value::as_str) {
        Some(n) => n.to_string(),
        None => return tool_err("missing tool name"),
    };
    if !state.rate.allow() {
        return finish(
            state,
            &name,
            &json!({}),
            Err(McpError::Denied("rate limit")),
        );
    }
    if let Some(expected) = &state.gateway_token {
        if state.caller_token.as_deref() != Some(expected.as_str()) {
            return finish(
                state,
                &name,
                &json!({}),
                Err(McpError::Denied("gateway token mismatch")),
            );
        }
    }
    let mut args = match params.get("arguments") {
        None | Some(Value::Null) => json!({}),
        Some(Value::Object(_)) => params["arguments"].clone(),
        Some(_) => {
            return finish(
                state,
                &name,
                &json!({}),
                Err(McpError::Denied("arguments must be object")),
            )
        }
    };
    if let Err(err) = verify_optional_sig(state, &mut args) {
        return finish(state, &name, &args, Err(err));
    }
    let result = dispatch(state, &name, &args);
    finish(state, &name, &args, result)
}

fn dispatch(state: &McpState, name: &str, args: &Value) -> Result<Value, McpError> {
    match name {
        "list_devices" => {
            reject_unknown(args, &[])?;
            require_scope(state, "devices:list")?;
            let devices: Vec<Value> = (state.list_devices)()
                .into_iter()
                .map(|d| json!({"path": d.path, "line": d.display_line()}))
                .collect();
            Ok(json!({"devices": devices}))
        }
        "list_logs" => {
            reject_unknown(args, &[])?;
            require_scope(state, "logs:read")?;
            list_log_files(state)
        }
        "search_logs" => {
            reject_unknown(args, &["query", "limit", "file"])?;
            require_scope(state, "logs:read")?;
            search_logs(state, args)
        }
        "read_log" => {
            reject_unknown(args, &["file", "max_lines"])?;
            require_scope(state, "logs:read")?;
            read_log(state, args)
        }
        "mcp_inventory" => {
            reject_unknown(args, &[])?;
            require_scope(state, "inventory:read")?;
            Ok(json!({
                "server_id": SERVER_NAME,
                "version": SERVER_VERSION,
                "transport": "stdio",
                "bind": "stdio (no tcp)",
                "sandbox": {
                    "network": "none",
                    "write_tools": false,
                    "log_dir_allowlist": true
                },
                "tools": tool_fingerprints(),
                "signing": {
                    "key_id": KEY_ID,
                    "required": state.require_signing
                },
                "require_scopes": state.require_scopes
            }))
        }
        _ => Err(McpError::Denied("unknown tool")),
    }
}

fn list_log_files(state: &McpState) -> Result<Value, McpError> {
    let entries = fs::read_dir(&state.log_dir).map_err(deny_dir)?;
    let mut files = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if name.starts_with("mcp-audit") || !allowed_name(name) {
            continue;
        }
        let bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
        files.push(json!({"name": name, "bytes": bytes}));
    }
    files.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    Ok(json!({"files": files}))
}

fn search_logs(state: &McpState, args: &Value) -> Result<Value, McpError> {
    let query = args.get("query").and_then(Value::as_str).unwrap_or("");
    if query.is_empty() || query.len() > 256 {
        return Err(McpError::Denied("query must be 1-256 characters"));
    }
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(20)
        .clamp(1, 50) as usize;
    let only = args.get("file").and_then(Value::as_str);
    let names = match only {
        Some(name) => vec![name.to_string()],
        None => {
            let listing = list_log_files(state)?;
            listing["files"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter_map(|f| f["name"].as_str().map(str::to_string))
                .collect()
        }
    };
    let mut matches = Vec::new();
    let mut truncated = false;
    for name in names {
        let path = resolve_log_file(&state.log_dir, &name)?;
        let text = read_capped(&path)?;
        for (i, line) in text.lines().enumerate() {
            if !line.contains(query) {
                continue;
            }
            if matches.len() >= limit {
                truncated = true;
                break;
            }
            matches.push(json!({"file": name, "line": i + 1, "text": line}));
        }
        if truncated {
            break;
        }
    }
    Ok(json!({"matches": matches, "truncated": truncated}))
}

fn read_log(state: &McpState, args: &Value) -> Result<Value, McpError> {
    let name = args
        .get("file")
        .and_then(Value::as_str)
        .ok_or(McpError::Denied("file required"))?;
    let max_lines = args
        .get("max_lines")
        .and_then(Value::as_u64)
        .unwrap_or(50)
        .clamp(1, 200) as usize;
    let path = resolve_log_file(&state.log_dir, name)?;
    let text = read_capped(&path)?;
    let all: Vec<&str> = text.lines().collect();
    let start = all.len().saturating_sub(max_lines);
    let lines: Vec<&str> = all[start..].to_vec();
    Ok(json!({"file": name, "lines": lines, "truncated": start > 0}))
}

fn resolve_log_file(log_dir: &Path, name: &str) -> Result<PathBuf, McpError> {
    if !allowed_name(name) {
        return Err(McpError::Denied("file not allowed"));
    }
    let joined = log_dir.join(name);
    if !joined.exists() {
        return Err(McpError::Denied("file not found"));
    }
    let root = fs::canonicalize(log_dir).map_err(deny_dir)?;
    let canon = fs::canonicalize(&joined).map_err(deny_dir)?;
    if !canon.starts_with(&root) {
        return Err(McpError::Denied("file not allowed"));
    }
    Ok(canon)
}

fn allowed_name(name: &str) -> bool {
    if name.is_empty() || name.starts_with('.') || name.contains('/') || name.contains('\\') {
        return false;
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    {
        return false;
    }
    let ext = name.rsplit('.').next().unwrap_or("");
    ALLOWED_EXT.contains(&ext)
}

fn read_capped(path: &Path) -> Result<String, McpError> {
    let meta = fs::metadata(path).map_err(deny_read)?;
    if meta.len() > MAX_FILE_BYTES {
        return Err(McpError::Denied("file exceeds 8 MiB cap"));
    }
    fs::read_to_string(path).map_err(deny_read)
}

fn require_scope(state: &McpState, scope: &str) -> Result<(), McpError> {
    if state.scopes.contains(scope) {
        Ok(())
    } else {
        Err(McpError::Denied("insufficient scope"))
    }
}

fn reject_unknown(args: &Value, allowed: &[&str]) -> Result<(), McpError> {
    let Some(obj) = args.as_object() else {
        return Err(McpError::Denied("arguments must be object"));
    };
    for key in obj.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(McpError::Message(format!("unknown field: {key}")));
        }
    }
    Ok(())
}

fn verify_optional_sig(state: &mut McpState, args: &mut Value) -> Result<(), McpError> {
    let obj = args
        .as_object_mut()
        .ok_or(McpError::Denied("arguments must be object"))?;
    let sig = obj.remove("_sig");
    if sig.is_none() && state.require_signing {
        return Err(McpError::Denied("signed envelope required"));
    }
    let Some(sig) = sig else { return Ok(()) };
    let key = state
        .hmac_key
        .as_deref()
        .ok_or(McpError::Denied("signing key not configured"))?;
    let env = Envelope {
        payload: sig["payload"].as_str().unwrap_or("").into(),
        signature: sig["signature"].as_str().unwrap_or("").into(),
        key_id: sig["key_id"].as_str().unwrap_or("").into(),
        nonce: sig["nonce"].as_str().unwrap_or("").into(),
        expires_at: sig["expires_at"].as_str().unwrap_or("").into(),
    };
    let canonical = serde_json::to_string(args).unwrap_or_default();
    if env.payload != canonical {
        return Err(McpError::Denied("payload mismatch"));
    }
    verify_envelope(&env, key, &mut state.replay)
}

pub fn sign_envelope(payload: &str, key: &[u8], nonce: &str, expires_at: &str) -> Envelope {
    Envelope {
        payload: payload.into(),
        signature: hmac_hex(key, &mac_bytes(nonce, expires_at, payload)),
        key_id: KEY_ID.into(),
        nonce: nonce.into(),
        expires_at: expires_at.into(),
    }
}

pub fn verify_envelope(
    env: &Envelope,
    key: &[u8],
    replay: &mut ReplayCache,
) -> Result<(), McpError> {
    if env.key_id != KEY_ID {
        return Err(McpError::Denied("unknown key id"));
    }
    let exp = DateTime::parse_from_rfc3339(&env.expires_at)
        .map_err(|_| McpError::Denied("invalid expiry"))?;
    if exp.with_timezone(&Utc) <= Utc::now() {
        return Err(McpError::Denied("message expired"));
    }
    if !replay.seen.insert(env.nonce.clone()) {
        return Err(McpError::Denied("replay detected"));
    }
    let expected = hmac_hex(key, &mac_bytes(&env.nonce, &env.expires_at, &env.payload));
    if expected != env.signature {
        replay.seen.remove(&env.nonce);
        return Err(McpError::Denied("invalid signature"));
    }
    Ok(())
}

fn finish(
    state: &mut McpState,
    tool: &str,
    params: &Value,
    result: Result<Value, McpError>,
) -> Value {
    let (status, body) = match result {
        Ok(v) => ("success", tool_ok(v)),
        Err(err) => ("denied", tool_err(&err.as_str())),
    };
    let text = body["content"][0]["text"].as_str().unwrap_or("");
    if let Err(err) = audit(state, tool, params, status, text) {
        return tool_err(&err.as_str());
    }
    body
}

fn audit(
    state: &mut McpState,
    tool: &str,
    params: &Value,
    status: &str,
    result_text: &str,
) -> Result<(), McpError> {
    let params_json = serde_json::to_string(params).unwrap_or_default();
    let mut event = json!({
        "timestamp": Utc::now().to_rfc3339(),
        "event_type": "mcp.tool.result",
        "server_id": SERVER_NAME,
        "tool_name": tool,
        "tool_version": "1.0.0",
        "caller_identity": state.caller,
        "authorization_scope": match tool {
            "list_devices" => "devices:list",
            "mcp_inventory" => "inventory:read",
            _ => "logs:read",
        },
        "params_fingerprint": format!("sha256:{}", hex_encode(&Sha256::digest(params_json.as_bytes()))),
        "result_status": status,
        "result_bytes": result_text.len(),
        "result_hash": format!("sha256:{}", hex_encode(&Sha256::digest(result_text.as_bytes()))),
        "prev_hash": state.prev_hash,
    });
    let without_hash = serde_json::to_string(&event).unwrap_or_default();
    let event_hash = format!(
        "sha256:{}",
        hex_encode(&Sha256::digest(without_hash.as_bytes()))
    );
    event["event_hash"] = json!(event_hash.clone());
    let line = serde_json::to_string(&event).unwrap_or_default();
    if let Some(path) = &state.audit_path {
        let mut opts = OpenOptions::new();
        opts.create(true).append(true);
        #[cfg(unix)]
        opts.mode(0o600);
        let mut file = opts.open(path).map_err(deny_audit)?;
        writeln!(file, "{line}").map_err(deny_audit)?;
    }
    state.prev_hash = event_hash;
    Ok(())
}

fn tool_ok(value: Value) -> Value {
    let mut text = serde_json::to_string(&value).unwrap_or_default();
    if text.len() > MAX_RESPONSE_BYTES {
        text = json!({"truncated": true, "message": "response exceeded 65536 bytes"}).to_string();
    }
    text = sanitize_output(&text);
    json!({"content":[{"type":"text","text": text}], "isError": false})
}

fn tool_err(msg: &str) -> Value {
    json!({"content":[{"type":"text","text": msg}], "isError": true})
}

pub fn sanitize_output(input: &str) -> String {
    let mut t = redact_prefix(input, "AKIA", 16);
    t = redact_kv(&t, "password=");
    t = redact_kv(&t, "token=");
    t = redact_kv(&t, "api_key=");
    for pem in [
        "-----BEGIN RSA PRIVATE KEY-----",
        "-----BEGIN PRIVATE KEY-----",
        "-----BEGIN OPENSSH PRIVATE KEY-----",
    ] {
        t = t.replace(pem, "[redacted-pem]");
    }
    replace_ignore_case(&t, "ignore previous instructions", "[redacted-instruction]")
}

fn redact_prefix(s: &str, prefix: &str, n: usize) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if s[i..].starts_with(prefix) {
            let rest = &s[i + prefix.len()..];
            let take = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .count();
            if take >= n {
                out.push_str("[redacted-secret]");
                i += prefix.len() + rest.chars().take(take).map(char::len_utf8).sum::<usize>();
                continue;
            }
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn redact_kv(s: &str, key: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if s[i..].starts_with(key) {
            out.push_str(key);
            out.push_str("[redacted]");
            i += key.len();
            while i < s.len() && !s[i..].chars().next().unwrap().is_whitespace() {
                i += s[i..].chars().next().unwrap().len_utf8();
            }
            continue;
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn replace_ignore_case(s: &str, pat: &str, repl: &str) -> String {
    let lower = s.to_ascii_lowercase();
    let pat_l = pat.to_ascii_lowercase();
    let mut out = String::new();
    let mut i = 0;
    while i < s.len() {
        if lower[i..].starts_with(&pat_l) {
            out.push_str(repl);
            i += pat.len();
        } else {
            let ch = s[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

pub fn tool_fingerprints() -> Vec<Value> {
    tool_defs()
        .into_iter()
        .map(|tool| {
            let canon = serde_json::to_string(&tool).unwrap_or_default();
            json!({
                "name": tool["name"],
                "version": "1.0.0",
                "fingerprint": format!("sha256:{}", hex_encode(&Sha256::digest(canon.as_bytes())))
            })
        })
        .collect()
}

fn tool_defs() -> Vec<Value> {
    vec![
        tool_def(
            "list_devices",
            "List USB serial devices (read-only).",
            json!({"type":"object","additionalProperties":false,"properties":{}}),
        ),
        tool_def(
            "list_logs",
            "List capture log files in the allowlisted log directory.",
            json!({"type":"object","additionalProperties":false,"properties":{}}),
        ),
        tool_def(
            "search_logs",
            "Search allowlisted capture logs for a literal string.",
            json!({
                "type":"object",
                "additionalProperties":false,
                "required":["query"],
                "properties":{
                    "query":{"type":"string","minLength":1,"maxLength":256},
                    "limit":{"type":"integer","minimum":1,"maximum":50},
                    "file":{"type":"string","maxLength":255}
                }
            }),
        ),
        tool_def(
            "read_log",
            "Read the last lines of one allowlisted capture log.",
            json!({
                "type":"object",
                "additionalProperties":false,
                "required":["file"],
                "properties":{
                    "file":{"type":"string","maxLength":255},
                    "max_lines":{"type":"integer","minimum":1,"maximum":200}
                }
            }),
        ),
        tool_def(
            "mcp_inventory",
            "Return pinned tool fingerprints and transport inventory.",
            json!({"type":"object","additionalProperties":false,"properties":{}}),
        ),
    ]
}

fn tool_def(name: &str, description: &str, schema: Value) -> Value {
    json!({"name": name, "description": description, "inputSchema": schema})
}

fn parse_hmac_key(s: &str) -> Vec<u8> {
    if s.len().is_multiple_of(2) && !s.is_empty() && s.bytes().all(|b| b.is_ascii_hexdigit()) {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    } else {
        s.as_bytes().to_vec()
    }
}

fn mac_bytes(nonce: &str, expires_at: &str, payload: &str) -> Vec<u8> {
    format!("{nonce}\n{expires_at}\n{payload}").into_bytes()
}

fn hmac_hex(key: &[u8], data: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(key).unwrap();
    mac.update(data);
    hex_encode(&mac.finalize().into_bytes())
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn deny_dir<E>(_: E) -> McpError {
    McpError::Denied("log directory not available")
}

fn deny_read<E>(_: E) -> McpError {
    McpError::Denied("file not readable")
}

fn deny_audit<E>(_: E) -> McpError {
    McpError::Denied("audit write failed")
}

fn rpc_ok(id: Value, result: Value) -> String {
    json!({"jsonrpc":"2.0","id":id,"result":result}).to_string()
}

fn rpc_error(id: Value, code: i64, message: &str) -> String {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}}).to_string()
}

#[cfg(test)]
#[path = "mcp_tests.rs"]
mod mcp_tests;
