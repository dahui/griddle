//! Tighten a module finder that has gone ambiguous. **Read-only.**
//!
//! ```powershell
//! cargo run -p sgdb-core --example cdp_disambiguate -- FocusableFactory
//! cargo run -p sgdb-core --example cdp_disambiguate -- FocusableFactory --token ReactCurrentOwner
//! ```
//!
//! When a finder matches more than one module, the answer is never "take the first" — that
//! freezes a coin-flip into the settings file. The answer is a tighter predicate, and this is
//! how you find one: it prints each candidate's size and the text around the anchor, so a
//! discriminator can be chosen from what is actually there rather than from memory.
//!
//! `--token` tests extra substrings against every candidate, which is the fastest way to check
//! whether a proposed discriminator actually separates them.

use sgdb_core::cdp::modules::{FINDERS, Finder};
use sgdb_core::cdp::{Endpoint, SteamJs};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(name) = args.first().filter(|a| !a.starts_with("--")) else {
        eprintln!("usage: cdp_disambiguate <FinderName> [--token STR]... [--context N]");
        eprintln!(
            "finders: {}",
            FINDERS
                .iter()
                .map(|f| f.name)
                .collect::<Vec<_>>()
                .join(", ")
        );
        std::process::exit(2);
    };
    let Some(finder) = FINDERS.iter().find(|f| f.name == name) else {
        eprintln!("no finder called {name:?}");
        std::process::exit(2);
    };

    let tokens: Vec<String> = args
        .iter()
        .enumerate()
        .filter(|(_, a)| a.as_str() == "--token")
        .filter_map(|(i, _)| args.get(i + 1).cloned())
        .collect();
    let context: usize = args
        .iter()
        .position(|a| a == "--context")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(160);

    let http = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("http client: {e}");
            std::process::exit(1);
        }
    };

    let (mut steam, readiness) = match SteamJs::connect(&http, &Endpoint::default()).await {
        Ok(pair) => pair,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    println!("build {:?}\n", readiness.clstamp);
    println!("finder {name}");
    println!("  all_of : {:?}", finder.all_of);
    println!("  note   : {}\n", finder.note);

    let script = probe_script(finder, &tokens, context);
    let report: serde_json::Value = match steam.connection().evaluate(&script).await {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    let Some(candidates) = report.get("candidates").and_then(|c| c.as_array()) else {
        println!("{report:#}");
        return;
    };
    println!("{} candidate module(s)\n", candidates.len());

    for c in candidates {
        let id = c.get("id").and_then(|v| v.as_str()).unwrap_or("?");
        let len = c
            .get("len")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        println!("── module {id}  ({len} bytes)");

        if let Some(hits) = c.get("tokens").and_then(|t| t.as_object()) {
            for (token, present) in hits {
                let mark = if present.as_bool().unwrap_or(false) {
                    "yes"
                } else {
                    "no "
                };
                println!("   [{mark}] {token}");
            }
        }
        if let Some(excerpt) = c.get("excerpt").and_then(|v| v.as_str()) {
            println!("   …{}…", excerpt.replace('\n', " "));
        }
        println!();
    }

    println!(
        "Pick a token present in exactly one candidate and add it to that finder's `all_of`, \n\
         or one present only in the wrong ones and add it to `none_of`."
    );
}

/// Collect each candidate's size, an excerpt around the first anchor, and which probe tokens
/// it contains.
fn probe_script(finder: &Finder, tokens: &[String], context: usize) -> String {
    let all_of = serde_json::to_string(finder.all_of).unwrap_or_else(|_| "[]".into());
    let none_of = serde_json::to_string(finder.none_of).unwrap_or_else(|_| "[]".into());
    let tokens = serde_json::to_string(tokens).unwrap_or_else(|_| "[]".into());
    format!(
        r#"(() => {{
  let req = null;
  try {{
    // A fresh chunk id per call: webpack keys installed chunks by id, and a literal `{{}}`
    // stringifies to "[object Object]" so only the first push of a session would work.
    const marker = '__sgdb_probe_' + Math.random().toString(36).slice(2);
    window.webpackChunksteamui.push([[marker], {{}}, (r) => {{ req = r; }}]);
  }}
  catch (e) {{ return {{ error: String(e) }}; }}
  if (!req || !req.m) return {{ error: 'no module registry' }};

  const ALL = {all_of}, NONE = {none_of}, TOKENS = {tokens}, CTX = {context};
  const candidates = [];
  for (const id of Object.keys(req.m)) {{
    let src = '';
    try {{ src = req.m[id].toString(); }} catch (e) {{ continue; }}
    if (ALL.some((s) => !src.includes(s))) continue;
    if (NONE.some((s) => src.includes(s))) continue;

    const at = src.indexOf(ALL[0]);
    const from = Math.max(0, at - Math.floor(CTX / 2));
    const tokenHits = {{}};
    for (const t of TOKENS) tokenHits[t] = src.includes(t);
    candidates.push({{
      id,
      len: src.length,
      excerpt: src.slice(from, from + CTX),
      tokens: tokenHits,
    }});
  }}
  return {{ candidates }};
}})()"#
    )
}
