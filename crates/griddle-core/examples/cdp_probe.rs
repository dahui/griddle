//! M1 spike harness — talks to a live Steam client over the CEF debugger.
//!
//! ```powershell
//! cargo run -p griddle-core --example cdp_probe             # env: realm, apply API, CSP
//! cargo run -p griddle-core --example cdp_probe -- --status  # no connection, just report state
//! cargo run -p griddle-core --example cdp_probe -- --apply   # S3 live apply — WRITES, see below
//! ```
//!
//! The harness never creates the sentinel, never restarts Steam, and never calls a `Set*` API —
//! enabling anything is a separate, explicit act. **`--apply` is the exception: it replaces real
//! artwork.** Back up `userdata/<id>/config/grid/` and restore it afterwards, verifying by hash;
//! that directory can hold art a user curated by hand and cannot regenerate.
//!
//! Everything here is destined for `griddle-core::cdp`, so it is written to be promoted rather
//! than thrown away: the target-selection rules and the "is this actually Steam" check below
//! are the real ones.
//!
//! Fourteen further probes existed during the spike and were deleted once their findings were
//! recorded in CLAUDE.md — including the dead ends (detached React roots, name-based module
//! searches), which are written up there specifically so they are not retried.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

const DEBUG_HOST: &str = "127.0.0.1";
const DEBUG_PORT: u16 = 8080;

fn main() {
    let status_only = std::env::args().any(|a| a == "--status");

    println!("== environment ==");
    let steam_path = steam_path();
    match &steam_path {
        Some(p) => println!("  steam root      : {p}"),
        None => println!("  steam root      : NOT FOUND"),
    }

    let sentinel = steam_path
        .as_ref()
        .map(|p| std::path::Path::new(p).join(".cef-enable-remote-debugging"));
    match &sentinel {
        Some(s) => println!(
            "  sentinel        : {} ({})",
            if s.exists() { "present" } else { "ABSENT" },
            s.display()
        ),
        None => println!("  sentinel        : unknown (no steam root)"),
    }

    let port_open = TcpStream::connect_timeout(
        &format!("{DEBUG_HOST}:{DEBUG_PORT}")
            .parse()
            .unwrap_or_else(|_| unreachable!()),
        Duration::from_millis(500),
    )
    .is_ok();
    println!(
        "  port {DEBUG_PORT}        : {}",
        if port_open { "open" } else { "closed" }
    );

    if !port_open {
        println!("\nCEF debugging is not active. To enable it:");
        println!("  1. create an empty file `.cef-enable-remote-debugging` in the Steam root");
        println!("  2. fully restart Steam (`steam.exe -shutdown`, wait for exit, relaunch)");
        return;
    }
    if status_only {
        return;
    }

    println!("\n== targets ==");
    let targets_json = match http_get(&format!("http://{DEBUG_HOST}:{DEBUG_PORT}/json")) {
        Ok(body) => body,
        Err(e) => {
            println!("  failed to list targets: {e}");
            return;
        }
    };

    let targets: Vec<serde_json::Value> = match serde_json::from_str(&targets_json) {
        Ok(t) => t,
        Err(e) => {
            println!("  /json was not a JSON array: {e}");
            println!("  Something other than Steam is listening on {DEBUG_PORT}. Not injecting.");
            return;
        }
    };

    for t in &targets {
        println!(
            "  [{}] {}  {}",
            t.get("type").and_then(|v| v.as_str()).unwrap_or("?"),
            t.get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("<untitled>"),
            t.get("url").and_then(|v| v.as_str()).unwrap_or(""),
        );
    }

    let Some(shared) = pick_shared_js_context(&targets) else {
        println!("\n  no SharedJSContext target found — is this really Steam on {DEBUG_PORT}?");
        return;
    };
    let Some(ws_url) = shared.get("webSocketDebuggerUrl").and_then(|v| v.as_str()) else {
        println!("\n  SharedJSContext has no webSocketDebuggerUrl");
        return;
    };

    println!("\n== probing SharedJSContext ==");
    println!("  {ws_url}");

    match run_probe(ws_url) {
        Ok(result) => {
            println!("\n== results ==");
            match serde_json::to_string_pretty(&result) {
                Ok(s) => println!("{s}"),
                Err(e) => println!("  (could not format result: {e})"),
            }
        }
        Err(e) => println!("\n  probe failed: {e}"),
    }
}

/// Target selection, in the order `griddle-core::cdp` will use it.
///
/// The exact-title match is first because it is unambiguous; the fallbacks cover title
/// changes across Steam builds. If none match we refuse rather than guess — evaluating
/// arbitrary JS against whatever happens to be on port 8080 would be reckless, and 8080 is a
/// very common dev-server port.
fn pick_shared_js_context(targets: &[serde_json::Value]) -> Option<&serde_json::Value> {
    let title = |t: &serde_json::Value| {
        t.get("title")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string()
    };
    let url = |t: &serde_json::Value| {
        t.get("url")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_lowercase()
    };

    targets
        .iter()
        .find(|t| title(t) == "SharedJSContext")
        .or_else(|| targets.iter().find(|t| url(t).contains("sharedjscontext")))
        .or_else(|| {
            targets.iter().find(|t| {
                t.get("type").and_then(|v| v.as_str()) == Some("page")
                    && url(t).contains("steamloopback.host")
            })
        })
}

fn run_probe(ws_url: &str) -> Result<serde_json::Value, String> {
    let (mut socket, _) =
        tungstenite::connect(ws_url).map_err(|e| format!("websocket connect: {e}"))?;

    let args: Vec<String> = std::env::args().collect();
    let selected = |name: &str| args.iter().any(|a| a == name);

    // The probes that survive; the dead ends were removed once their findings were recorded in
    // CLAUDE.md. Each of these is still worth re-running after a Steam update.
    //
    // `--modules`, `--bpm` and `--menu` went with the Big Picture deliverable: they resolved
    // Steam's own React components by structural search, which nothing in this product now does.
    // Their findings, and the five wrong turns that produced them, remain in CLAUDE.md.
    let probe_js = if selected("--apply") {
        include_str!("apply.js") // S3: live artwork apply (WRITES — back up grid/ first)
    } else if selected("--animated") {
        include_str!("animated.js") // S4: animated WebP/APNG labelled "png" (WRITES)
    } else if selected("--enum") {
        include_str!("asset_enum.js") // de-mangle ELibraryAssetType
    } else if selected("--icon") {
        include_str!("icon.js") // S8: Icon asset type for a real Steam app (WRITES)
    } else {
        include_str!("env.js") // default: realm, apply API, CSP, webpack discovery
    };

    let evaluate = |id: u64, expr: &str| {
        serde_json::json!({
            "id": id,
            "method": "Runtime.evaluate",
            "params": {
                "expression": expr,
                "returnByValue": true,
                "awaitPromise": true,
                // The probe reads globals Steam defines; it must run in the page's own world.
                "includeCommandLineAPI": false,
            }
        })
        .to_string()
    };

    // `--payload <file>` injects a base64 blob as `window.__SGDB_PAYLOAD__` before the probe
    // runs. This exists because **SharedJSContext cannot read image bytes itself**: a normal
    // `fetch()` to cdn2.steamgriddb.com is CORS-blocked (only `mode:'no-cors'` succeeds, and
    // that response is opaque). Images can be *displayed* from the CDN but not *read*.
    //
    // That is precisely why decky-steamgriddb has a Python `download_as_base64` backend, and
    // it matches our architecture: Rust owns the SGDB client and hands base64 across.
    if let Some(i) = args.iter().position(|a| a == "--payload") {
        let path = args.get(i + 1).ok_or("--payload needs a file path")?;
        let b64 = std::fs::read_to_string(path).map_err(|e| format!("read payload: {e}"))?;
        let b64 = b64.trim();
        let expr = format!(
            "window.__SGDB_PAYLOAD__ = {}; window.__SGDB_PAYLOAD__.length",
            serde_json::Value::String(b64.to_string())
        );
        socket
            .send(tungstenite::Message::Text(evaluate(90, &expr).into()))
            .map_err(|e| format!("send payload: {e}"))?;
        let n = read_evaluate_result(&mut socket, 90)?;
        println!("  injected payload: {n} base64 chars");
    }

    // `--appid <n>` selects the app a write probe targets.
    if let Some(i) = args.iter().position(|a| a == "--appid") {
        let id = args.get(i + 1).ok_or("--appid needs a number")?;
        let expr = format!("window.__SGDB_APPID__ = {id}");
        socket
            .send(tungstenite::Message::Text(evaluate(91, &expr).into()))
            .map_err(|e| format!("send appid: {e}"))?;
        let _ = read_evaluate_result(&mut socket, 91)?;
        println!("  target appid: {id}");
    }

    if let Some(i) = args.iter().position(|a| a == "--assettype") {
        let n = args.get(i + 1).ok_or("--assettype needs a number")?;
        let expr = format!("window.__SGDB_ASSET_TYPE__ = {n}");
        socket
            .send(tungstenite::Message::Text(evaluate(92, &expr).into()))
            .map_err(|e| format!("send assettype: {e}"))?;
        let _ = read_evaluate_result(&mut socket, 92)?;
    }

    socket
        .send(tungstenite::Message::Text(evaluate(1, probe_js).into()))
        .map_err(|e| format!("send: {e}"))?;

    let mut result = read_evaluate_result(&mut socket, 1)?;

    // The image and fetch checks are asynchronous — give them a moment, then read the flags
    // the probe parks on `window`.
    let async_results = "JSON.stringify({\
        sgdb: String(window.__sgdbProbeImage), \
        control: String(window.__sgdbProbeControl), \
        fetch: String(window.__sgdbProbeFetch)})";

    std::thread::sleep(Duration::from_millis(2500));
    socket
        .send(tungstenite::Message::Text(
            evaluate(2, async_results).into(),
        ))
        .map_err(|e| format!("send async check: {e}"))?;

    if let Ok(v) = read_evaluate_result(&mut socket, 2)
        && let Some(obj) = result.get_mut("sections").and_then(|s| s.get_mut("csp"))
        && let Some(map) = obj.as_object_mut()
    {
        let parsed = v
            .as_str()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
            .unwrap_or(v);
        map.insert("asyncResults".into(), parsed);
    }

    // Closing is courtesy — the probe's result is already in hand — but the workspace lints
    // forbid discarding a Result silently, so say something if it fails.
    if let Err(e) = socket.close(None) {
        eprintln!("  (note: websocket close failed: {e})");
    }
    Ok(result)
}

fn read_evaluate_result(
    socket: &mut tungstenite::WebSocket<impl Read + Write>,
    want_id: u64,
) -> Result<serde_json::Value, String> {
    // CDP interleaves events with responses; skip anything that isn't the reply we asked for.
    for _ in 0..200 {
        let msg = socket.read().map_err(|e| format!("read: {e}"))?;
        let tungstenite::Message::Text(text) = msg else {
            continue;
        };
        let v: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| format!("bad frame: {e}"))?;

        if v.get("id").and_then(|i| i.as_u64()) != Some(want_id) {
            continue;
        }
        if let Some(err) = v.get("error") {
            return Err(format!("CDP error: {err}"));
        }
        let res = v.get("result").and_then(|r| r.get("result"));
        if let Some(thrown) = v.get("result").and_then(|r| r.get("exceptionDetails")) {
            return Err(format!("probe threw: {thrown}"));
        }
        return Ok(res
            .and_then(|r| r.get("value"))
            .cloned()
            .unwrap_or(serde_json::Value::Null));
    }
    Err("no matching response after 200 frames".into())
}

/// Minimal HTTP/1.1 GET. Loopback, plain HTTP, one small JSON body — a full HTTP client would
/// be a heavier dependency than the task deserves.
///
/// **`Content-Length` is honoured rather than reading to EOF.** Steam's CEF DevTools HTTP
/// server ignores `Connection: close` and holds the socket open after responding, so
/// `read_to_end` blocks until the read timeout and the request looks like a connection
/// failure even though the server answered immediately.
/// `[VERIFIED-BOX 2026-07-27 — cost one confusing os error 10060]`
fn http_get(url: &str) -> Result<String, String> {
    let rest = url
        .strip_prefix("http://")
        .ok_or("only http:// supported")?;
    let (host_port, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };

    let mut stream = TcpStream::connect(host_port).map_err(|e| format!("connect: {e}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .map_err(|e| format!("set timeout: {e}"))?;
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: {host_port}\r\nAccept: application/json\r\nConnection: close\r\n\r\n"
    )
    .map_err(|e| format!("write: {e}"))?;

    // Read until the header terminator, then exactly `Content-Length` more bytes.
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    let header_end = loop {
        let n = stream
            .read(&mut byte)
            .map_err(|e| format!("read header: {e}"))?;
        if n == 0 {
            return Err("connection closed before headers completed".into());
        }
        buf.push(byte[0]);
        if buf.len() >= 4 && &buf[buf.len() - 4..] == b"\r\n\r\n" {
            break buf.len();
        }
        if buf.len() > 64 * 1024 {
            return Err("header exceeded 64 KiB".into());
        }
    };

    let headers = String::from_utf8_lossy(&buf[..header_end]).to_ascii_lowercase();
    let content_length: usize = headers
        .lines()
        .find_map(|l| l.strip_prefix("content-length:"))
        .and_then(|v| v.trim().parse().ok())
        .ok_or("response had no Content-Length")?;

    let mut body = vec![0u8; content_length];
    stream
        .read_exact(&mut body)
        .map_err(|e| format!("read body: {e}"))?;
    String::from_utf8(body).map_err(|e| format!("body was not UTF-8: {e}"))
}

#[cfg(windows)]
fn steam_path() -> Option<String> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = hkcu.open_subkey(r"Software\Valve\Steam").ok()?;
    let raw: String = key.get_value("SteamPath").ok()?;
    // HKCU stores this lowercased with forward slashes (`c:/program files (x86)/steam`).
    // [VERIFIED-BOX 2026-07-27]
    Some(raw.replace('/', "\\"))
}

#[cfg(not(windows))]
fn steam_path() -> Option<String> {
    None
}
