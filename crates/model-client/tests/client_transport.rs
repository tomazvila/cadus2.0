//! The transport of the client: the endpoint rules of the constructor, and
//! every way one HTTP attempt reaches no status.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::time::Duration;

use cadus_model_client::{Client, HttpClient, ModelError, TransportError};
use common::{FakeModel, Reply, chat_request, local_config};
use serde_json::json;

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

/// A server that closes the socket without a reply, and a reply past the
/// one-megabyte bound, both end the attempt with the transport error of that
/// step, and the call retries them.
#[tokio::test]
async fn a_hangup_and_an_oversized_reply_are_transport_errors() {
    let server = FakeModel::start_with(vec![Reply::Hangup, Reply::Hangup]).await;
    let call = server.client().call(&chat_request()).await;
    assert_eq!(server.seen().len(), 2);
    match call.result {
        Err(ModelError::Transport(why)) => {
            assert!(why.starts_with("the request failed: "), "{why}");
        }
        other => panic!("a hangup must give a transport error, it gave {other:?}"),
    }

    let huge = "x".repeat((1 << 20) + 1);
    let raw = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: text/plain\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{huge}",
        huge.len()
    );
    let server = FakeModel::start_with(vec![Reply::Raw(raw.clone()), Reply::Raw(raw)]).await;
    let mut cfg = local_config(&server.base_url);
    cfg.timeout = Duration::from_secs(20);
    let call = Client::new(cfg).unwrap().call(&chat_request()).await;
    match call.result {
        Err(ModelError::Transport(why)) => {
            assert!(why.starts_with("the reply did not arrive whole: "), "{why}");
        }
        other => panic!("an oversized reply must give a transport error, it gave {other:?}"),
    }
}
