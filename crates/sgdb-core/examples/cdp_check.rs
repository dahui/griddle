//! Exercise the `cdp` module against the running Steam client. **Read-only.**
//!
//! ```powershell
//! cargo run -p sgdb-core --example cdp_check
//! ```
//!
//! Reports the sentinel state, finds `SharedJSContext`, feature-detects the artwork API, and
//! cross-checks the build stamp against `steamui/changelist.txt` on disk. It applies nothing
//! and writes nothing.
//!
//! The spike harness (`examples/cdp_probe.rs`) stays as the exploratory tool with its own JS
//! payloads; this one proves the shipped library works, so what ships is what was tested.

use sgdb_core::appid::AppId;
use sgdb_core::cdp::{Endpoint, Sentinel, SteamJs, target};
use sgdb_core::steam::locate;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let install = match locate::locate() {
        Ok(i) => i,
        Err(e) => {
            eprintln!("locate Steam: {e}");
            std::process::exit(1);
        }
    };
    println!("steam: {}", install.root().display());

    println!("\n== sentinel ==");
    let sentinel = Sentinel::for_install(&install);
    let state = sentinel.state();
    println!("  {}", sentinel.path().display());
    println!("  state: {state:?}");
    println!("  {}", state.explain());

    println!("\n== build stamp on disk ==");
    match install.clstamp_from_disk() {
        Some(s) => println!("  changelist.txt: {s}"),
        None => println!("  changelist.txt: unreadable"),
    }

    let http = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("http client: {e}");
            std::process::exit(1);
        }
    };
    let endpoint = Endpoint::default();

    println!("\n== debugger ==");
    match target::fetch_version(&http, &endpoint).await {
        Ok(v) => {
            println!("  browser  : {}", v.browser);
            println!("  protocol : {}", v.protocol_version);
            println!("  is steam : {}", v.looks_like_steam());
        }
        Err(e) => {
            println!("  {e}");
            std::process::exit(1);
        }
    }

    println!("\n== targets ==");
    match target::fetch_targets(&http, &endpoint).await {
        Ok(targets) => {
            println!("  {} target(s)", targets.len());
            for t in &targets {
                let marker = if t.is_shared_js_context() { "<<<" } else { "" };
                println!("    [{}] {:<28} {marker}", t.kind, t.title);
            }
        }
        Err(e) => println!("  {e}"),
    }

    println!("\n== connect + probe ==");
    let (mut steam, readiness) = match SteamJs::connect(&http, &endpoint).await {
        Ok(pair) => pair,
        Err(e) => {
            eprintln!("  {e}");
            std::process::exit(1);
        }
    };
    println!("  CLSTAMP (live) : {:?}", readiness.clstamp);
    println!("  apply API      : {}", readiness.apply_api);
    println!("  webpack hook   : {}", readiness.webpack);
    println!("  can apply live : {}", readiness.can_apply());
    println!("  can inject UI  : {}", readiness.can_inject_ui());

    // The cross-check the module map depends on.
    println!("\n== build stamp: page vs disk ==");
    let disk = install.clstamp_from_disk();
    match (&readiness.clstamp, &disk) {
        (Some(live), Some(d)) if live == d => println!("  ✅ both read {live}"),
        (Some(live), Some(d)) => println!("  🔴 live {live} != disk {d}"),
        (live, d) => println!("  incomplete: live {live:?}, disk {d:?}"),
    }

    // Confirm the unsigned appid is what the JS side understands. This is the form that
    // crosses the CDP boundary — the signed form lives only inside shortcuts.vdf.
    println!("\n== appStore lookup (unsigned appid) ==");
    for id in [4_048_848_997u32, 620] {
        match steam.app_name(AppId::new(id)).await {
            Ok(Some(name)) => println!("  {id:>10} -> {name}"),
            Ok(None) => println!("  {id:>10} -> (not in appStore)"),
            Err(e) => println!("  {id:>10} -> {e}"),
        }
    }

    // Prove a JS exception surfaces as an error rather than being swallowed. If this ever
    // reports success, `evaluate` is reading past `exceptionDetails`.
    println!("\n== a deliberate JS exception must be reported ==");
    match steam
        .connection()
        .evaluate::<serde_json::Value>("window.__sgdb_no_such_function__()")
        .await
    {
        Err(e) => println!("  ✅ reported: {e}"),
        Ok(v) => println!("  🔴 a throw was NOT reported; got {v:?}"),
    }

    println!("\nread-only: nothing was applied or written");
}
