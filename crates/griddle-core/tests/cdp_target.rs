#![allow(
    clippy::expect_used,
    clippy::let_underscore_must_use,
    reason = "a failing test should panic with a message, and the fake server deliberately \
              ignores socket errors from a client that has already got what it needed"
)]
//! End-to-end checks that `cdp::target` refuses to talk to anything that is not Steam.
//!
//! Port 8080 is one of the most commonly occupied ports on a developer's machine. If discovery
//! ever accepted a Vite dev server, a Jenkins or a Tomcat, this app would evaluate JavaScript
//! into somebody else's process. The unit tests cover the predicate; these cover the whole
//! path, against a real socket, including the case where the listener answers politely with
//! completely the wrong thing.
//!
//! The fake server is ~30 lines of hand-rolled HTTP rather than a test-server dependency —
//! enough to answer two GETs, which is all discovery performs.

use griddle_core::cdp::target::{self, Endpoint};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpListener;

/// Serve a fixed body for `/json/version` and `/json`, then exit.
///
/// Returns the port it bound to. Port 0 lets the OS pick a free one, so these tests never
/// collide with a real Steam or with each other.
async fn fake_server(version_body: &'static str, targets_body: &'static str) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind an ephemeral port");
    let port = listener.local_addr().expect("local addr").port();

    tokio::spawn(async move {
        // Discovery makes at most two requests; serve a few more in case of retries.
        for _ in 0..4 {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let mut buf = [0u8; 2048];
            let Ok(n) = socket.read(&mut buf).await else {
                continue;
            };
            let request = String::from_utf8_lossy(&buf[..n]).into_owned();

            let body = if request.contains("/json/version") {
                version_body
            } else {
                targets_body
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\
                 Content-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = socket.write_all(response.as_bytes()).await;
            let _ = socket.shutdown().await;
        }
    });

    port
}

fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .expect("build a client")
}

#[tokio::test]
async fn a_dev_server_on_the_port_is_refused_rather_than_injected_into() {
    // What a Vite/webpack dev server would look like: valid JSON, wrong software.
    let port = fake_server(
        r#"{"Browser":"Werkzeug/3.0.1","Protocol-Version":"1.1","User-Agent":"Werkzeug"}"#,
        "[]",
    )
    .await;

    let err = target::discover(&http(), &Endpoint::new("127.0.0.1", port))
        .await
        .expect_err("a non-Steam listener must be refused");

    match &err {
        target::Error::NotSteam { port: p, .. } => assert_eq!(*p, port),
        other => panic!("expected NotSteam, got {other:?}"),
    }
    // The message has to tell the user what to do, because this is a configuration problem on
    // their machine and not a bug in the app.
    let msg = err.to_string();
    assert!(msg.contains("not Steam"), "{msg}");
    assert!(msg.contains("development-server port"), "{msg}");
}

#[tokio::test]
async fn a_listener_serving_html_is_refused() {
    // Plenty of things answer any path with a web page. `/json/version` returning HTML must
    // not crash or be misread as a Steam handshake.
    let port = fake_server("<!DOCTYPE html><html><body>hello</body></html>", "[]").await;

    let err = target::discover(&http(), &Endpoint::new("127.0.0.1", port))
        .await
        .expect_err("HTML is not a CDP handshake");
    assert!(matches!(err, target::Error::NotSteam { .. }), "{err:?}");
}

#[tokio::test]
async fn a_chrome_with_remote_debugging_on_the_same_port_is_refused() {
    // The nastiest case: genuinely CDP, genuinely a browser, genuinely not Steam. Every
    // structural check passes except the identity of the client itself.
    //
    // The JSON is deliberately on one line. In a Rust *raw* string a trailing `\` is literal,
    // so a wrapped user-agent produces invalid JSON — which made this test pass for the wrong
    // reason (a parse failure rather than the identity check) until the control test caught it.
    let port = fake_server(
        r#"{"Browser":"Chrome/126.0.6478.183","Protocol-Version":"1.3","User-Agent":"Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36"}"#,
        r#"[{"id":"A","title":"SharedJSContext","type":"page","url":"https://steamloopback.host/index.html","webSocketDebuggerUrl":"ws://127.0.0.1:1/devtools/page/A"}]"#,
    )
    .await;

    let err = target::discover(&http(), &Endpoint::new("127.0.0.1", port))
        .await
        .expect_err("a plain Chrome must not be mistaken for Steam");
    assert!(
        matches!(err, target::Error::NotSteam { .. }),
        "even a target that looks perfect must be rejected when the client is not Steam: {err:?}"
    );
}

#[tokio::test]
async fn a_steam_like_listener_with_no_shared_js_context_is_reported_distinctly() {
    // Steam is up but still starting: the realm has not appeared yet. That is a different
    // situation from "this is not Steam", and the user should be told to wait, not to close
    // another application.
    let port = fake_server(
        r#"{"Browser":"Chrome/126.0.6478.183","Protocol-Version":"1.3","User-Agent":"Mozilla/5.0 Valve Steam Client/default/0 Safari/537.36"}"#,
        r#"[{"id":"B","title":"Steam Big Picture Mode","type":"page","url":"https://steamloopback.host/routes/bpm","webSocketDebuggerUrl":"ws://127.0.0.1:1/devtools/page/B"}]"#,
    )
    .await;

    let err = target::discover(&http(), &Endpoint::new("127.0.0.1", port))
        .await
        .expect_err("no SharedJSContext in the list");
    assert!(
        matches!(err, target::Error::NoSharedJsContext),
        "{err:?} — must be distinguishable from NotSteam"
    );
}

#[tokio::test]
async fn a_real_steam_handshake_resolves_the_shared_js_context() {
    // The control. Without it, every test above would pass against a `discover` that simply
    // always failed. Bodies are the shapes measured on this machine.
    // `[VERIFIED-BOX @ CLSTAMP 10840511, 2026-07-27]`
    let port = fake_server(
        r#"{"Browser":"Chrome/126.0.6478.183","Protocol-Version":"1.3","User-Agent":"Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Valve Steam Client/default/0 Safari/537.36"}"#,
        r#"[{"id":"MENU","title":"Menu","type":"page","url":"https://steamloopback.host/contextmenu","webSocketDebuggerUrl":"ws://127.0.0.1:1/devtools/page/MENU"},{"id":"SJC","title":"SharedJSContext","type":"page","url":"https://steamloopback.host/index.html?debug=1","webSocketDebuggerUrl":"ws://127.0.0.1:1/devtools/page/SJC"}]"#,
    )
    .await;

    let found = target::discover(&http(), &Endpoint::new("127.0.0.1", port))
        .await
        .expect("a real Steam handshake must resolve");
    assert_eq!(found.id, "SJC");
    assert!(found.websocket_url.starts_with("ws://"));
}

#[tokio::test]
async fn nothing_listening_is_reported_as_steam_not_running() {
    // Bind and immediately drop, so the port is almost certainly free and definitely not ours.
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    drop(listener);

    let err = target::discover(&http(), &Endpoint::new("127.0.0.1", port))
        .await
        .expect_err("nothing is listening");
    match &err {
        target::Error::NotListening { .. } => {}
        other => panic!("expected NotListening, got {other:?}"),
    }
    // This is the common case for a user who simply has not started Steam, so the message
    // must not read like a failure of the app.
    let msg = err.to_string();
    assert!(msg.contains("Steam must be running"), "{msg}");
}
