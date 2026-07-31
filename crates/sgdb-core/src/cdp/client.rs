//! A minimal Chrome DevTools Protocol client — enough to evaluate JavaScript in Steam's realm.
//!
//! Only `Runtime.evaluate` and `Page.addScriptToEvaluateOnNewDocument` are implemented. This is
//! not a general CDP library and should not become one; the smaller it is, the less there is to
//! break when Valve ships a new CEF.
//!
//! # Requests and events share one socket
//!
//! CDP interleaves responses (`{"id":N,"result":…}`) with events (`{"method":"…","params":…}`).
//! Every request carries a monotonic id and [`Connection::send`] reads until it sees a matching
//! one, discarding events. That is correct for the request/response use here; if the supervisor
//! later needs to observe events, this is the place that grows a dispatcher rather than every
//! caller growing a workaround.
//!
//! # A JS exception is a value, not a transport failure
//!
//! `Runtime.evaluate` returns HTTP-200-equivalent success even when the script threw:
//! the throw appears in `exceptionDetails`. Treating that as success is how "the call silently
//! did nothing" bugs happen, so [`Error::JsException`] is a distinct, loud error.

use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use futures_util::{SinkExt as _, StreamExt as _};

/// How long to wait for a single evaluation. Generous: an apply takes ~30 ms, but the icon
/// ordinal was measured taking ~500 ms before doing nothing at all.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("connecting to {url}: {source}")]
    Connect {
        url: String,
        #[source]
        source: Box<tokio_tungstenite::tungstenite::Error>,
    },

    #[error("the debugger connection closed unexpectedly")]
    Closed,

    #[error("websocket error: {0}")]
    Socket(String),

    #[error("the debugger did not answer within {0:?}")]
    Timeout(Duration),

    /// The script ran and threw. Distinct from a transport failure on purpose.
    #[error("JavaScript threw in Steam's realm: {0}")]
    JsException(String),

    #[error("could not understand the debugger's reply: {0}")]
    Decode(String),

    #[error("the evaluation returned no value")]
    NoValue,
}

/// An open CDP session against one target.
pub struct Connection {
    socket: WebSocketStream<MaybeTlsStream<TcpStream>>,
    next_id: u64,
    timeout: Duration,
}

#[derive(Debug, Deserialize)]
struct Response {
    #[serde(default)]
    id: Option<u64>,
    #[serde(default)]
    result: Option<serde_json::Value>,
    #[serde(default)]
    error: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct EvaluateResult {
    #[serde(default)]
    result: Option<RemoteObject>,
    #[serde(rename = "exceptionDetails", default)]
    exception_details: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct RemoteObject {
    #[serde(default)]
    value: Option<serde_json::Value>,
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    description: Option<String>,
}

impl Connection {
    /// Open a session against a target's `webSocketDebuggerUrl`.
    pub async fn connect(ws_url: &str) -> Result<Self, Error> {
        let (socket, _response) =
            tokio_tungstenite::connect_async(ws_url)
                .await
                .map_err(|source| Error::Connect {
                    url: ws_url.to_owned(),
                    source: Box::new(source),
                })?;
        Ok(Connection {
            socket,
            next_id: 1,
            timeout: DEFAULT_TIMEOUT,
        })
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Evaluate an expression and deserialise its value.
    ///
    /// `awaitPromise` is set, so an expression returning a promise resolves before we read it —
    /// `SetCustomArtworkForApp` returns one, and without this the call would appear to succeed
    /// instantly while the work was still in flight.
    pub async fn evaluate<T: DeserializeOwned>(&mut self, expression: &str) -> Result<T, Error> {
        let value = self.evaluate_value(expression).await?;
        serde_json::from_value(value).map_err(|e| Error::Decode(e.to_string()))
    }

    /// Evaluate for side effects, discarding any result.
    pub async fn evaluate_unit(&mut self, expression: &str) -> Result<(), Error> {
        let _ = self.evaluate_value(expression).await?;
        Ok(())
    }

    async fn evaluate_value(&mut self, expression: &str) -> Result<serde_json::Value, Error> {
        let params = serde_json::json!({
            "expression": expression,
            "returnByValue": true,
            "awaitPromise": true,
            // Steam's own code runs in this realm. Without this, a `let` at top level would
            // collide with an earlier evaluation's binding and throw on re-injection.
            "replMode": true,
        });

        let result = self.send("Runtime.evaluate", params).await?;
        let eval: EvaluateResult =
            serde_json::from_value(result).map_err(|e| Error::Decode(e.to_string()))?;

        // Checked before the value: a thrown exception still produces a `result` (an error
        // object), so reading the value first would silently return the exception as data.
        if let Some(details) = eval.exception_details {
            return Err(Error::JsException(summarise_exception(&details)));
        }

        let object = eval.result.ok_or(Error::NoValue)?;
        Ok(match object.value {
            Some(v) => v,
            // `undefined` has no `value` field at all. Map it to JSON null so callers can
            // deserialise into `()` or `Option<T>` rather than treating it as a failure —
            // `SetCustomArtworkForApp` returns undefined on success.
            None if object.kind == "undefined" => serde_json::Value::Null,
            None => {
                return Err(Error::Decode(format!(
                    "no value on a {} result ({})",
                    object.kind,
                    object.description.unwrap_or_default()
                )));
            }
        })
    }

    /// Register a script to run on every future document load in this target.
    ///
    /// Survives `SharedJSContext` reloading itself, which it does on some Steam navigations.
    /// Paired with one immediate `evaluate` for the current document.
    pub async fn add_script_on_new_document(&mut self, source: &str) -> Result<String, Error> {
        let result = self
            .send(
                "Page.addScriptToEvaluateOnNewDocument",
                serde_json::json!({ "source": source }),
            )
            .await?;
        Ok(result
            .get("identifier")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned())
    }

    /// Send one command and wait for the response with the matching id.
    async fn send(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, Error> {
        let id = self.next_id;
        self.next_id += 1;

        let message = serde_json::json!({ "id": id, "method": method, "params": params });
        self.socket
            .send(Message::Text(message.to_string().into()))
            .await
            .map_err(|e| Error::Socket(e.to_string()))?;

        tokio::time::timeout(self.timeout, self.read_response(id))
            .await
            .map_err(|_| Error::Timeout(self.timeout))?
    }

    /// Read until the response with `id` arrives, discarding events and other traffic.
    async fn read_response(&mut self, id: u64) -> Result<serde_json::Value, Error> {
        loop {
            let message = self.socket.next().await.ok_or(Error::Closed)?;
            let message = message.map_err(|e| Error::Socket(e.to_string()))?;

            let text = match message {
                Message::Text(t) => t.to_string(),
                Message::Binary(b) => String::from_utf8_lossy(&b).into_owned(),
                Message::Close(_) => return Err(Error::Closed),
                // Ping/Pong/Frame are handled by the library or irrelevant.
                _ => continue,
            };

            let Ok(response) = serde_json::from_str::<Response>(&text) else {
                continue; // Not a response shape; an event we do not care about.
            };
            if response.id != Some(id) {
                continue;
            }
            if let Some(err) = response.error {
                return Err(Error::Decode(err.to_string()));
            }
            return response.result.ok_or(Error::NoValue);
        }
    }
}

/// Pull something legible out of CDP's `exceptionDetails`, which nests the useful part.
fn summarise_exception(details: &serde_json::Value) -> String {
    details
        .get("exception")
        .and_then(|e| e.get("description"))
        .and_then(|d| d.as_str())
        .or_else(|| details.get("text").and_then(|t| t.as_str()))
        .unwrap_or("unknown error")
        .to_owned()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "test assertions are allowed to panic")]
mod tests {
    use super::*;

    #[test]
    fn an_exception_is_summarised_from_the_nested_description() {
        let details = serde_json::json!({
            "exceptionId": 1,
            "text": "Uncaught",
            "exception": {
                "type": "object",
                "className": "TypeError",
                "description": "TypeError: SteamClient.Apps.Nope is not a function"
            }
        });
        assert_eq!(
            summarise_exception(&details),
            "TypeError: SteamClient.Apps.Nope is not a function"
        );
    }

    #[test]
    fn an_exception_without_a_description_falls_back_to_the_text() {
        let details = serde_json::json!({ "text": "Uncaught (in promise)" });
        assert_eq!(summarise_exception(&details), "Uncaught (in promise)");
        assert_eq!(summarise_exception(&serde_json::json!({})), "unknown error");
    }

    #[test]
    fn a_thrown_exception_is_detected_even_though_a_result_is_present() {
        // The trap this guards: CDP reports a throw *alongside* a populated `result`, so
        // reading the value first would hand the caller the exception object as if it were
        // data. Verified against the real shape of a failed evaluate.
        let raw = serde_json::json!({
            "result": { "type": "object", "subtype": "error",
                        "className": "TypeError", "description": "TypeError: x is not a function" },
            "exceptionDetails": {
                "text": "Uncaught",
                "exception": { "description": "TypeError: x is not a function" }
            }
        });
        let eval: EvaluateResult = serde_json::from_value(raw).unwrap();
        assert!(eval.result.is_some(), "a result really is present");
        assert!(
            eval.exception_details.is_some(),
            "and so are the exception details, which must win"
        );
    }

    #[test]
    fn undefined_deserialises_as_a_successful_unit_result() {
        // `SetCustomArtworkForApp` returns undefined on success — measured at 28 ms against a
        // real shortcut. Treating a missing `value` as a failure would make every successful
        // apply look broken.
        let raw = serde_json::json!({ "result": { "type": "undefined" } });
        let eval: EvaluateResult = serde_json::from_value(raw).unwrap();
        let object = eval.result.unwrap();
        assert_eq!(object.kind, "undefined");
        assert!(object.value.is_none());
    }

    #[test]
    fn ordinary_values_carry_through() {
        let raw = serde_json::json!({
            "result": { "type": "string", "value": "10840511" }
        });
        let eval: EvaluateResult = serde_json::from_value(raw).unwrap();
        assert_eq!(
            eval.result.unwrap().value.unwrap(),
            serde_json::Value::String("10840511".into())
        );
    }

    #[test]
    fn a_response_without_an_id_is_an_event_not_a_reply() {
        let event: Response =
            serde_json::from_str(r#"{"method":"Runtime.consoleAPICalled","params":{}}"#).unwrap();
        assert_eq!(event.id, None);

        let reply: Response = serde_json::from_str(r#"{"id":7,"result":{"ok":true}}"#).unwrap();
        assert_eq!(reply.id, Some(7));
        assert!(reply.result.is_some());
    }
}
