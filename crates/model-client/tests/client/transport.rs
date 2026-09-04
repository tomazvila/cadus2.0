//! The transport of the client: the endpoint rules of the constructor, and
//! every way one HTTP attempt reaches no status.

use std::sync::Arc;
use std::time::Duration;

use cadus_model_client::{Call, Client, HttpClient, ModelError, TransportError};
use rustls::RootCertStore;
use serde_json::json;

use crate::common::{FakeModel, Reply, chat_request, local_config, ring_provider, test_roots};

/// The transport error of one attempt against `base_url`, with `cfg` as the
/// configuration of the call.
async fn attempt_error(base_url: &str, cfg: &cadus_model_client::ModelConfig) -> String {
    let http = HttpClient::new(base_url).unwrap();
    http.post_json(cfg, &json!({}))
        .await
        .unwrap_err()
        .to_string()
}

/// The constructor refuses a base URL that does not parse, one that names no
/// host, and one of a scheme that is neither http nor https; it accepts both
/// schemes, and a client built over a refused URL is a configuration error.
#[test]
fn the_constructor_holds_the_endpoint_rules() {
    let refused = |base: &str| HttpClient::new(base).unwrap_err().to_string();
    assert_eq!(
        refused("not a url"),
        "the base URL does not parse: relative URL without a base"
    );
    assert_eq!(refused("data:text"), "the base URL names no host");
    assert_eq!(
        refused("ftp://models.example/v1"),
        "the base URL scheme \"ftp\" is neither http nor https"
    );
    let long_label = format!("https://{}.example/v1", "a".repeat(64));
    assert!(
        refused(&long_label).starts_with("the host is not a TLS server name: "),
        "{}",
        refused(&long_label)
    );
    assert!(HttpClient::new("http://models.example/v1/").is_ok());
    assert!(HttpClient::new("https://models.example/v1").is_ok());
    let mut no_suites = rustls::crypto::ring::default_provider();
    no_suites.cipher_suites.clear();
    let no_tls = HttpClient::with_trust(
        "https://models.example/v1",
        Arc::new(no_suites),
        RootCertStore::empty(),
    );
    assert_eq!(
        no_tls.unwrap_err().to_string(),
        "the TLS protocol versions did not build: unexpected error: no usable cipher suites \
         configured"
    );
    assert_eq!(
        TransportError("x".to_owned()),
        TransportError("x".to_owned())
    );

    let err = Client::new(local_config("not a url")).unwrap_err();
    assert_eq!(
        err.to_string(),
        "model configuration error: the base URL does not parse: relative URL without a base"
    );
    let client = Client::new(local_config("http://127.0.0.1:1/v1")).unwrap();
    assert_eq!(client.config().base_url, "http://127.0.0.1:1/v1");
}

/// A port that nobody listens on, a bearer that is not a header value, and a
/// server that speaks no TLS each end the attempt before a byte of the
/// request is sent, and each names its step.
#[tokio::test]
async fn the_attempt_names_the_step_that_failed_before_the_send() {
    let cfg = local_config("http://127.0.0.1:1/v1");
    let refused = attempt_error("http://127.0.0.1:1/v1", &cfg).await;
    assert!(refused.starts_with("the connection failed: "), "{refused}");

    let mut bad_bearer = cfg.clone();
    bad_bearer.api_key = "line\nbreak".to_owned();
    let unbuilt = attempt_error("http://127.0.0.1:1/v1", &bad_bearer).await;
    assert!(
        unbuilt.starts_with("the request does not build: "),
        "{unbuilt}"
    );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((socket, _)) = listener.accept().await {
            drop(socket);
        }
    });
    let no_tls = attempt_error(&format!("https://127.0.0.1:{port}/v1"), &cfg).await;
    assert!(no_tls.starts_with("the TLS handshake failed: "), "{no_tls}");
}

/// The transport error of `call`, or a panic that names the other outcome.
fn transport_error(call: Call) -> String {
    match call.result {
        Err(ModelError::Transport(why)) => why,
        other => panic!("the call must give a transport error, it gave {other:?}"),
    }
}

/// One call of `chat_request` against a server that answers `raw` twice,
/// with a twenty-second bound.
async fn call_raw_twice(raw: String) -> Call {
    let server = FakeModel::start_with(vec![Reply::Raw(raw.clone()), Reply::Raw(raw)]).await;
    let mut cfg = local_config(&server.base_url);
    cfg.timeout = Duration::from_secs(20);
    Client::new(cfg).unwrap().call(&chat_request()).await
}

/// A server that closes the socket without a reply, a reply past the
/// one-megabyte bound, and a reply that ends before its content length, each
/// end the attempt with the transport error of that step, and the call
/// retries them.
#[tokio::test]
async fn a_hangup_and_a_broken_reply_are_transport_errors() {
    let server = FakeModel::start_with(vec![Reply::Hangup, Reply::Hangup]).await;
    let call = server.client().call(&chat_request()).await;
    assert_eq!(server.seen().len(), 2);
    let why = transport_error(call);
    assert!(why.starts_with("the request failed: "), "{why}");

    let huge = "x".repeat((1 << 20) + 1);
    let raw = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: text/plain\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{huge}",
        huge.len()
    );
    let why = transport_error(call_raw_twice(raw).await);
    assert!(why.starts_with("the reply did not arrive whole: "), "{why}");

    let short = "HTTP/1.1 200 OK\r\ncontent-length: 10\r\nconnection: close\r\n\r\nabc".to_owned();
    let why = transport_error(call_raw_twice(short).await);
    assert!(why.starts_with("the reply did not arrive whole: "), "{why}");
}

/// A keep-alive reply followed by bytes that no request asked for ends the
/// connection task with an error, and the attempt keeps its status and body.
#[tokio::test]
async fn unrequested_bytes_after_the_reply_end_the_connection_alone() {
    let raw = "HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\n{}HTTP/1.1 500 X\r\n\r\n".to_owned();
    let server = FakeModel::start_with(vec![Reply::Raw(raw)]).await;
    let http = HttpClient::new(&server.base_url).unwrap();
    let cfg = local_config(&server.base_url);
    let (status, reply) = http.post_json(&cfg, &json!({})).await.unwrap();
    assert_eq!((status, reply), (200, b"{}".to_vec()));
    tokio::task::yield_now().await;
}

/// An `https` endpoint answers through the TLS handshake when the trust
/// store holds the authority of the server certificate.
#[tokio::test]
async fn a_tls_endpoint_answers_through_the_handshake() {
    let server = FakeModel::start_tls(vec![Reply::Status(200, "{\"ok\":true}".to_owned())]).await;
    let http = HttpClient::with_trust(&server.base_url, ring_provider(), test_roots()).unwrap();
    let cfg = local_config(&server.base_url);
    let (status, reply) = http.post_json(&cfg, &json!({"a": 1})).await.unwrap();
    assert_eq!(status, 200);
    assert_eq!(reply, b"{\"ok\":true}");
    let seen = server.seen();
    assert_eq!(seen.len(), 1);
    assert!(
        seen[0]
            .head
            .starts_with("post /v1/chat/completions http/1.1\r\n"),
        "{}",
        seen[0].head
    );
    assert_eq!(seen[0].body, json!({"a": 1}));
}

/// The completions path lands after the last character of the base URL that
/// is not a slash: on the path, on the query, or on the fragment. A fragment
/// is not part of the request line.
#[tokio::test]
async fn the_completions_path_lands_on_the_tail_of_the_base_url() {
    for (tail, request_line) in [
        ("/", "post /v1/chat/completions http/1.1"),
        ("?x=/", "post /v1?x=/chat/completions http/1.1"),
        ("#f/", "post /v1 http/1.1"),
    ] {
        let server = FakeModel::start(vec![(200, String::new())]).await;
        let base_url = format!("{}{tail}", server.base_url);
        let http = HttpClient::new(&base_url).unwrap();
        let cfg = local_config(&base_url);
        http.post_json(&cfg, &json!({})).await.unwrap();
        let head = server.seen()[0].head.clone();
        assert!(head.starts_with(request_line), "{tail}: {head}");
    }
}
