//! Tests for [`super`]. Split out to keep the implementation readable on its own.

use super::*;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const FAKE_KEY: &str = "0123456789abcdef0123456789abcdef";

async fn client_for(server: &MockServer) -> Client {
    Client::with_config(
        ApiKey::new(FAKE_KEY).unwrap(),
        Config {
            base_url: format!("{}/api/v2", server.uri()),
            // Keep the tests fast; the retry *logic* is what is under test, not the wait.
            backoff_base: Duration::from_millis(1),
            timeout: Duration::from_secs(5),
            ..Default::default()
        },
    )
    .unwrap()
}

fn grid_body() -> serde_json::Value {
    serde_json::json!({
        "success": true, "page": 0, "total": 424, "limit": 1,
        "data": [{"id": 103243, "url": "https://cdn2.steamgriddb.com/grid/a.png",
                  "thumb": "https://cdn2.steamgriddb.com/thumb/a.jpg",
                  "width": 600, "height": 900, "mime": "image/png",
                  "author": {"name": "Reiisen"}}]
    })
}

#[tokio::test]
async fn sends_the_bearer_token_and_parses_a_page() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v2/grids/steam/620"))
        .and(header(
            "authorization",
            format!("Bearer {FAKE_KEY}").as_str(),
        ))
        .and(query_param("dimensions", "600x900,342x482,660x930"))
        .respond_with(ResponseTemplate::new(200).set_body_json(grid_body()))
        .mount(&server)
        .await;

    let client = client_for(&server).await;
    let (kind, query) = AssetQuery::for_asset_type(crate::grid::names::AssetType::Capsule).unwrap();
    let page = client
        .assets(kind, Target::Steam(AppId::new(620)), &query)
        .await
        .unwrap();

    assert_eq!(page.total, 424);
    assert_eq!(page.assets.len(), 1);
    assert_eq!(page.assets[0].id, 103243);
}

#[tokio::test]
async fn a_401_is_reported_as_a_key_problem_and_is_not_retried() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(401))
        // The count assertion is the real test: retrying a bad key wastes the user's time
        // and cannot possibly succeed.
        .expect(1)
        .mount(&server)
        .await;

    let client = client_for(&server).await;
    let err = client.validate_key().await.unwrap_err();
    assert!(matches!(err, Error::Unauthorized), "{err:?}");

    // The message must name no screen. It used to say "Check it in Settings.", which is a dead
    // end on the first-run screen -- there is no Settings tab until a key exists -- and
    // redundant once the user is in Settings. The remedy belongs to whoever is showing the
    // error, not to the transport.
    assert!(!err.to_string().contains("Settings"), "{err}");
}

#[tokio::test]
async fn a_429_is_retried_with_backoff_and_then_succeeds() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(429))
        .up_to_n_times(2)
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(grid_body()))
        .expect(1)
        .mount(&server)
        .await;

    let client = client_for(&server).await;
    let page = client
        .assets(
            AssetKind::Grid,
            Target::Steam(AppId::new(620)),
            &AssetQuery::default(),
        )
        .await
        .unwrap();
    assert_eq!(page.assets.len(), 1);
}

#[tokio::test]
async fn retries_are_bounded_and_end_in_the_real_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(503))
        // 1 initial attempt + max_retries(3).
        .expect(4)
        .mount(&server)
        .await;

    let client = client_for(&server).await;
    let err = client.validate_key().await.unwrap_err();
    assert!(matches!(err, Error::Server { status: 503 }), "{err:?}");
}

#[tokio::test]
async fn an_html_404_page_is_not_parsed_as_json() {
    // A mis-built URL lands on the website's HTML 404. Verified real behaviour: the body
    // is a full page. It must never reach the user as an error message.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            "<!DOCTYPE html><html><head><title>404</title>".as_bytes(),
            "text/html; charset=utf-8",
        ))
        .mount(&server)
        .await;

    let client = client_for(&server).await;
    let err = client.validate_key().await.unwrap_err();
    match &err {
        Error::NotJson { content_type, .. } => assert!(content_type.contains("text/html")),
        other => panic!("expected NotJson, got {other:?}"),
    }
    assert!(
        !err.to_string().contains("DOCTYPE"),
        "the HTML body must not be echoed into the error: {err}"
    );
}

#[tokio::test]
async fn an_unknown_game_is_none_rather_than_an_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let client = client_for(&server).await;
    let got = client
        .game_by_steam_appid(AppId::new(999_999_999))
        .await
        .unwrap();
    assert_eq!(got, None, "a game SGDB has never heard of is not a failure");
}

#[tokio::test]
async fn a_search_term_with_url_characters_is_encoded_into_the_path() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        // If the term were interpolated raw, the `/` and `?` would rewrite the path and
        // this mock would never match.
        .and(path("/api/v2/search/autocomplete/Portal%202%3F%20%2Fslash"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"success": true, "data": []})),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = client_for(&server).await;
    let hits = client.search("Portal 2? /slash").await.unwrap();
    assert!(hits.is_empty());
}

#[tokio::test]
async fn an_empty_search_term_never_hits_the_network() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;

    let client = client_for(&server).await;
    assert!(client.search("   ").await.unwrap().is_empty());
}

#[tokio::test]
async fn downloads_carry_no_authorization_header() {
    // The key must not reach the CDN. A default header on the client would break this
    // silently, which is why auth is attached per request.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/grid/a.png"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![0x89, b'P', b'N', b'G']))
        .expect(1)
        .mount(&server)
        .await;

    let client = client_for(&server).await;
    let bytes = client
        .download(&format!("{}/grid/a.png", server.uri()))
        .await
        .unwrap();
    assert_eq!(bytes, vec![0x89, b'P', b'N', b'G']);

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    assert!(
        requests[0].headers.get("authorization").is_none(),
        "the API key must never be sent to the CDN"
    );
}

#[tokio::test]
async fn an_oversized_download_is_refused() {
    // Real bytes past a small configured limit, because **a mock cannot lie about
    // `content-length`**: hyper rejects a header that disagrees with the body, so a mock
    // advertising a false size exercises a broken server rather than the limit.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![0u8; 4096]))
        .mount(&server)
        .await;

    let client = Client::with_config(
        ApiKey::new(FAKE_KEY).unwrap(),
        Config {
            base_url: format!("{}/api/v2", server.uri()),
            max_download_bytes: 64,
            ..Default::default()
        },
    )
    .unwrap();

    let err = client
        .download(&format!("{}/huge.png", server.uri()))
        .await
        .unwrap_err();
    assert!(matches!(err, Error::DownloadTooLarge), "{err:?}");
}

#[tokio::test]
async fn a_download_within_the_limit_succeeds() {
    // The control for the test above: without it, a limit of zero would also "pass".
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![7u8; 32]))
        .mount(&server)
        .await;

    let client = Client::with_config(
        ApiKey::new(FAKE_KEY).unwrap(),
        Config {
            base_url: format!("{}/api/v2", server.uri()),
            max_download_bytes: 64,
            ..Default::default()
        },
    )
    .unwrap();

    let bytes = client
        .download(&format!("{}/ok.png", server.uri()))
        .await
        .unwrap();
    assert_eq!(bytes.len(), 32);
}

#[tokio::test]
async fn a_success_false_envelope_on_a_200_is_still_an_error() {
    // Not observed in the wild — this API signals failure with a status code — but the
    // flag is parsed, so it must be acted on rather than decoratively present.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "success": false, "errors": ["Game not found", "try again"]
        })))
        .mount(&server)
        .await;

    let client = client_for(&server).await;
    let err = client.validate_key().await.unwrap_err();
    match &err {
        Error::Api(detail) => assert_eq!(detail, "Game not found; try again"),
        other => panic!("expected Api, got {other:?}"),
    }
}

#[test]
fn only_transient_failures_are_retryable() {
    assert!(Error::RateLimited.is_retryable());
    assert!(Error::Timeout.is_retryable());
    assert!(Error::Server { status: 502 }.is_retryable());
    assert!(Error::Network("reset".into()).is_retryable());

    // Retrying any of these is pure waste — they will fail identically forever.
    assert!(!Error::Unauthorized.is_retryable());
    assert!(!Error::NotFound.is_retryable());
    assert!(!Error::BadRequest.is_retryable());
    assert!(!Error::Decode("bad".into()).is_retryable());
}

#[test]
fn backoff_grows_and_stays_bounded() {
    let c = Client::with_config(ApiKey::new(FAKE_KEY).unwrap(), Config::default()).unwrap();
    let d0 = c.backoff(0);
    let d3 = c.backoff(3);
    assert!(d0 >= Duration::from_millis(500));
    assert!(d3 >= Duration::from_millis(4000), "{d3:?}");
    assert!(d3 < Duration::from_secs(10), "{d3:?}");
    // A large attempt count must not overflow into a nonsensical wait.
    assert!(c.backoff(64) < Duration::from_secs(120));
}

#[test]
fn debug_output_never_contains_the_key() {
    let c = Client::with_config(ApiKey::new(FAKE_KEY).unwrap(), Config::default()).unwrap();
    let rendered = format!("{c:?}");
    assert!(!rendered.contains(FAKE_KEY), "{rendered}");
}
