//! The fake OpenAI-compatible server of the client tests, and the fixtures
//! every test sends.
//!
//! The endpoint is one `TcpListener` on `127.0.0.1`, a hand-written HTTP/1.1
//! reply per call, and a record of every request it received. No test reaches
//! a real provider. The `tests/client` binary is the one user of the module.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use cadus_model_client::{Call, ChatRequest, Client, ModelConfig, ToolSpec};
use rustls::crypto::CryptoProvider;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::{RootCertStore, ServerConfig};
use serde_json::{Value, json};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

/// The certificate authority of the TLS tests: ECDSA P-256, self-signed,
/// valid for one hundred years.
const TEST_CA_PEM: &str = "-----BEGIN CERTIFICATE-----
MIIBlzCCAT2gAwIBAgIUJATafsztSj1novMFd+V0BCvF5d4wCgYIKoZIzj0EAwIw
GDEWMBQGA1UEAwwNY2FkdXMgdGVzdCBDQTAgFw0yNjA5MDQwNTU1NDBaGA8yMTI2
MDgxMTA1NTU0MFowGDEWMBQGA1UEAwwNY2FkdXMgdGVzdCBDQTBZMBMGByqGSM49
AgEGCCqGSM49AwEHA0IABATEQ+/c/swx0/X3nU90QtNCw4CbIhBskaI33zt0iyrT
VJ+Y6ZG3IQAY9ErQvnQqZNarMsjm9/zRMJXPx942NjSjYzBhMB0GA1UdDgQWBBTE
5qHalbligEBE3JC5lK99j1RcfjAfBgNVHSMEGDAWgBTE5qHalbligEBE3JC5lK99
j1RcfjAPBgNVHRMBAf8EBTADAQH/MA4GA1UdDwEB/wQEAwICBDAKBggqhkjOPQQD
AgNIADBFAiAN2mRQoQq0ZG596HHm7zq+rGjnf88t7N5OHN5bhCQXVwIhAIXbTOvz
JpEDGM3tfjP3AidDZhZPyj0mUPzZi4fjvKAB
-----END CERTIFICATE-----
";

/// The server certificate of `127.0.0.1`, signed by the test authority.
const TEST_LEAF_PEM: &str = "-----BEGIN CERTIFICATE-----
MIIBsjCCAVegAwIBAgIUJl4Q7cPIiCEJ00Ek094KXiX+kcswCgYIKoZIzj0EAwIw
GDEWMBQGA1UEAwwNY2FkdXMgdGVzdCBDQTAgFw0yNjA5MDQwNTU1NDBaGA8yMTI2
MDgxMTA1NTU0MFowFDESMBAGA1UEAwwJMTI3LjAuMC4xMFkwEwYHKoZIzj0CAQYI
KoZIzj0DAQcDQgAElYEUuDbeo7LGWJWq0Fjr5FezQP1SbB8b8PzYR+RCcGdPvj18
F2zA9q9XcCvAe/6QphWOfURzuaB720T7k5IP6qOBgDB+MA8GA1UdEQQIMAaHBH8A
AAEwCQYDVR0TBAIwADALBgNVHQ8EBAMCB4AwEwYDVR0lBAwwCgYIKwYBBQUHAwEw
HQYDVR0OBBYEFAx7asq13RRO7K01wPZRi4zmcHI9MB8GA1UdIwQYMBaAFMTmodqV
uWKAQETckLmUr32PVFx+MAoGCCqGSM49BAMCA0kAMEYCIQDB2k+3V8TR/XFG7eK/
zYgYh42Ly+QixQvkeM1Oyv0rngIhAKbssKfPigxSeb+qekk17afYAYaCubVQskTp
+GBynX79
-----END CERTIFICATE-----
";

/// The PKCS#8 private key of the server certificate.
const TEST_LEAF_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgoppw70+dnnS4wiWC
NKKYGWIISISVNj/ieGWc8nRrBUOhRANCAASVgRS4Nt6jssZYlarQWOvkV7NA/VJs
Hxvw/NhH5EJwZ0++PXwXbMD2r1dwK8B7/pCmFY59RHO5oHvbRPuTkg/q
-----END PRIVATE KEY-----
";

/// The `ring` crypto provider.
pub fn ring_provider() -> Arc<CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

/// A trust store that holds the test authority alone.
pub fn test_roots() -> RootCertStore {
    let mut roots = RootCertStore::empty();
    roots
        .add(CertificateDer::from_pem_slice(TEST_CA_PEM.as_bytes()).unwrap())
        .unwrap();
    roots
}

/// The TLS acceptor of the fake server: the server certificate over `ring`.
fn acceptor() -> TlsAcceptor {
    let cert = CertificateDer::from_pem_slice(TEST_LEAF_PEM.as_bytes()).unwrap();
    let key = PrivateKeyDer::from_pem_slice(TEST_LEAF_KEY_PEM.as_bytes()).unwrap();
    let config = ServerConfig::builder_with_provider(ring_provider())
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(vec![cert], key)
        .unwrap();
    TlsAcceptor::from(Arc::new(config))
}

/// One request the fake server received.
#[derive(Debug, Clone)]
pub struct Seen {
    /// The request line and the headers, lowercased.
    pub head: String,
    /// The JSON body.
    pub body: Value,
}

/// What the fake server sends back to one request.
#[derive(Debug, Clone)]
pub enum Reply {
    /// An HTTP/1.1 reply with this status and this JSON body.
    Status(u16, String),
    /// These bytes, as they are.
    Raw(String),
    /// Nothing: the socket closes without a byte.
    Hangup,
}

/// A local endpoint that answers a fixed list of replies, in order.
pub struct FakeModel {
    pub base_url: String,
    seen: Arc<Mutex<Vec<Seen>>>,
}

/// Read one HTTP request from `socket`: the head, then the body that
/// `content-length` names.
async fn read_request<S: AsyncRead + Unpin>(socket: &mut S) -> String {
    let mut raw: Vec<u8> = Vec::new();
    let mut buffer = [0_u8; 4096];
    loop {
        let read = socket.read(&mut buffer).await.unwrap_or(0);
        if read == 0 {
            return String::from_utf8_lossy(&raw).to_string();
        }
        raw.extend_from_slice(&buffer[..read]);
        let text = String::from_utf8_lossy(&raw).to_string();
        if let Some(split) = text.find("\r\n\r\n") {
            let head = text[..split].to_lowercase();
            let length: usize = head
                .split("\r\n")
                .find_map(|line| line.strip_prefix("content-length:"))
                .and_then(|value| value.trim().parse().ok())
                .unwrap_or(0);
            if text.len() >= split + 4 + length {
                return text;
            }
        }
    }
}

/// The bytes of one reply.
fn reply_bytes(reply: Reply) -> Option<String> {
    match reply {
        Reply::Status(status, payload) => Some(format!(
            "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{payload}",
            payload.len()
        )),
        Reply::Raw(bytes) => Some(bytes),
        Reply::Hangup => None,
    }
}

/// Read one request from `socket`, record it in `record`, and send `reply`.
async fn answer<S: AsyncRead + AsyncWrite + Unpin>(
    mut socket: S,
    record: &Mutex<Vec<Seen>>,
    reply: Reply,
) {
    let request = read_request(&mut socket).await;
    let split = request.find("\r\n\r\n").unwrap_or(request.len());
    let head = request[..split].to_lowercase();
    let body: Value =
        serde_json::from_str(request.get(split + 4..).unwrap_or("")).unwrap_or(Value::Null);
    record.lock().unwrap().push(Seen { head, body });

    if let Some(bytes) = reply_bytes(reply) {
        let _ = socket.write_all(bytes.as_bytes()).await;
        let _ = socket.flush().await;
    }
}

impl FakeModel {
    /// Bind `127.0.0.1:0` and serve `replies` in order, one connection each.
    ///
    /// A call past the end of the list gets `500` with an empty body, so a test
    /// that expects three attempts and gets four still fails on the count.
    pub async fn start(replies: Vec<(u16, String)>) -> FakeModel {
        Self::start_with(
            replies
                .into_iter()
                .map(|(status, body)| Reply::Status(status, body))
                .collect(),
        )
        .await
    }

    /// `start` with any reply shape.
    pub async fn start_with(replies: Vec<Reply>) -> FakeModel {
        Self::serve(replies, None).await
    }

    /// `start_with` behind TLS: the endpoint is `https`, and the server
    /// presents the certificate that [`test_roots`] trusts.
    pub async fn start_tls(replies: Vec<Reply>) -> FakeModel {
        Self::serve(replies, Some(acceptor())).await
    }

    /// Bind `127.0.0.1:0`, with `tls` in front of every connection when it is
    /// given, and serve `replies` in order.
    async fn serve(replies: Vec<Reply>, tls: Option<TlsAcceptor>) -> FakeModel {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let scheme = if tls.is_some() { "https" } else { "http" };
        let seen = Arc::new(Mutex::new(Vec::new()));
        let record = Arc::clone(&seen);

        tokio::spawn(async move {
            let mut index = 0_usize;
            loop {
                let Ok((socket, _)) = listener.accept().await else {
                    return;
                };
                let reply = replies
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| Reply::Status(500, String::new()));
                index += 1;
                match &tls {
                    None => answer(socket, &record, reply).await,
                    Some(acceptor) => {
                        if let Ok(stream) = acceptor.accept(socket).await {
                            answer(stream, &record, reply).await;
                        }
                    }
                }
            }
        });

        FakeModel {
            base_url: format!("{scheme}://127.0.0.1:{port}/v1"),
            seen,
        }
    }

    /// The requests the server received, in order.
    pub fn seen(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }

    /// A client pointed at this endpoint, with the shipped T5 defaults.
    pub fn client(&self) -> Client {
        Client::new(local_config(&self.base_url)).unwrap()
    }

    /// One call of `chat_request` against this endpoint.
    pub async fn call(&self) -> Call {
        self.client().call(&chat_request()).await
    }
}

/// Start a server with `replies`, make one call, and return both.
pub async fn call_with(replies: Vec<(u16, String)>) -> (FakeModel, Call) {
    let server = FakeModel::start(replies).await;
    let call = server.call().await;
    (server, call)
}

/// Serve `first` and then a good reply, make one call, and check that the
/// call made exactly two attempts with the second at the 4x ceiling.
pub async fn widened_retry(first: String) -> Call {
    let (server, call) = call_with(vec![(200, first), (200, good_reply("tool_calls"))]).await;
    let seen = server.seen();
    assert_eq!(seen.len(), 2, "the call must make exactly two attempts");
    assert_eq!(seen[0].body["max_tokens"], json!(600));
    assert_eq!(seen[1].body["max_tokens"], json!(2400));
    call
}

/// The configuration of a local OpenAI-compatible endpoint.
pub fn local_config(base_url: &str) -> ModelConfig {
    ModelConfig {
        base_url: base_url.to_owned(),
        api_key: "test-key".to_owned(),
        model: "qwen3.6".to_owned(),
        output_tokens: 600,
        reasoning_max_tokens: 600,
        provider_order: Vec::new(),
        timeout: Duration::from_secs(5),
    }
}

/// The one request every test sends.
pub fn chat_request() -> ChatRequest {
    ChatRequest {
        system: "You are the grader for Cadus.".to_owned(),
        user: "Problem: 8 - 5".to_owned(),
        tool: ToolSpec {
            name: "emit_diagnosis".to_owned(),
            description: "Name the misconception.".to_owned(),
            // The required list is the schema of spec section 6.3. The client
            // reads it to decide truncation shape 3, so the fixture states it.
            parameters: json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["error_tags", "prose"],
                "properties": {
                    "error_tags": {"type": "array", "items": {"type": "string"}},
                    "prose": {"type": "string"},
                },
            }),
        },
    }
}

/// A reply that carries a complete forced tool call.
pub fn good_reply(finish: &str) -> String {
    json!({
        "id": "gen-1",
        "provider": "DeepInfra",
        "choices": [{
            "finish_reason": finish,
            "message": {"tool_calls": [{"function": {
                "name": "emit_diagnosis",
                "arguments": "{\"error_tags\":[\"sign-error\"],\"prose\":\"Watch the sign.\"}"
            }}]}
        }],
        "usage": {
            "prompt_tokens": 1000,
            "prompt_tokens_details": {"cached_tokens": 900},
            "completion_tokens": 120,
            "completion_tokens_details": {"reasoning_tokens": 80},
            "cost": 0.000123
        }
    })
    .to_string()
}
