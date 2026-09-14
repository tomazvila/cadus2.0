//! One HTTP/1.1 POST to an OpenAI-compatible endpoint.
//!
//! The transport is deliberately small: one connection per call, no pool, no
//! redirect following, and a bound on the response body. One model call per job
//! buys nothing from connection reuse, and every feature a pooling client adds
//! is one more thing between the worker and its bill (T6).
//!
//! TLS runs on `rustls` with the `ring` provider, which `sqlx` already resolves
//! for its own connections. A plain `http://` base URL — the local endpoint of
//! O2's later pivot — skips the handshake and opens the socket directly.

use std::sync::Arc;

use bytes::Bytes;
use http_body_util::{BodyExt, Full, Limited};
use hyper::Request;
use hyper::header::{AUTHORIZATION, CONTENT_TYPE, HOST};
use hyper_util::rt::TokioIo;
use rustls::crypto::CryptoProvider;
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, RootCertStore};
use serde_json::Value;
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use url::Url;

use crate::{ModelConfig, TITLE};

/// The path this client appends to the configured base URL.
const COMPLETIONS_PATH: &str = "/chat/completions";

/// The response bytes the client reads before it gives up.
///
/// A diagnosis is three sentences. A body past this bound is a wrong endpoint or
/// a hostile one, and reading it without end costs the worker its memory.
const MAX_REPLY_BYTES: usize = 1 << 20;

/// The `X-Title` header of every request (1.0 `openai_engine.py:634-641`).
const TITLE_HEADER: &str = "X-Title";

/// Why one HTTP attempt reached no status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportError(pub String);

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for TransportError {}

/// Turn any error into a transport error with its own words.
fn why(context: &str, err: &dyn std::fmt::Display) -> TransportError {
    TransportError(format!("{context}: {err}"))
}

/// The endpoint and, for an `https` base URL, the TLS setup.
#[derive(Debug)]
pub struct HttpClient {
    endpoint: Url,
    /// The host of the endpoint. `new` refuses a URL without one.
    host: String,
    /// The port of the endpoint, or the default port of its scheme.
    port: u16,
    /// The TLS setup and the server name of an `https` endpoint.
    tls: Option<(Arc<ClientConfig>, ServerName<'static>)>,
}

impl HttpClient {
    /// Parse the base URL and build the TLS setup once, with the webpki trust
    /// anchors and the `ring` provider.
    ///
    /// # Errors
    ///
    /// Returns every [`TransportError`] of [`HttpClient::with_trust`].
    pub fn new(base_url: &str) -> Result<Self, TransportError> {
        let mut roots = RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        Self::with_trust(base_url, provider, roots)
    }

    /// Build a transport for an exact internal URL, without the completion suffix.
    pub(crate) fn new_exact(url: &str) -> Result<Self, TransportError> {
        let mut client = Self::new(url)?;
        client.endpoint = Url::parse(url).map_err(|err| why("the URL does not parse", &err))?;
        Ok(client)
    }

    /// [`HttpClient::new`] over a given crypto provider and trust store.
    ///
    /// A test gives a trust store that holds its own certificate authority,
    /// and a provider with no cipher suite to reach the TLS setup error.
    ///
    /// # Errors
    ///
    /// Returns [`TransportError`] when the base URL does not parse, names no
    /// host, carries an unsupported scheme, names an `https` host that is not
    /// a TLS server name, or when the TLS protocol versions of `provider` do
    /// not build.
    pub fn with_trust(
        base_url: &str,
        provider: Arc<CryptoProvider>,
        roots: RootCertStore,
    ) -> Result<Self, TransportError> {
        let base = Url::parse(base_url).map_err(|err| why("the base URL does not parse", &err))?;
        let endpoint = endpoint_of(&base);
        let Some(host) = endpoint.host_str() else {
            return Err(TransportError("the base URL names no host".to_owned()));
        };
        let host = host.to_owned();
        let (tls, default_port) = match endpoint.scheme() {
            "http" => (None, 80),
            "https" => {
                let name = ServerName::try_from(host.clone())
                    .map_err(|err| why("the host is not a TLS server name", &err))?;
                (Some((tls_config(provider, roots)?, name)), 443)
            }
            other => {
                return Err(TransportError(format!(
                    "the base URL scheme {other:?} is neither http nor https"
                )));
            }
        };
        let port = endpoint.port().unwrap_or(default_port);
        Ok(Self {
            endpoint,
            host,
            port,
            tls,
        })
    }

    /// POST one JSON body and read the status and the reply bytes.
    ///
    /// The whole call runs inside `cfg.timeout`, so an endpoint that accepts the
    /// socket and then says nothing costs one bounded attempt (1.0's client
    /// timeout is 60 s, `openai_engine.py:125`).
    ///
    /// # Errors
    ///
    /// Returns [`TransportError`] when the connection, the handshake, the write
    /// or the read fails, and when the bound expires. Every one of them is
    /// retryable: the endpoint reported no status, so nothing about the request
    /// is known to be wrong.
    pub async fn post_json(
        &self,
        cfg: &ModelConfig,
        body: &Value,
    ) -> Result<(u16, Vec<u8>), TransportError> {
        let call = self.send(cfg, body);
        match tokio::time::timeout(cfg.timeout, call).await {
            Ok(answer) => answer,
            Err(_) => Err(TransportError(format!(
                "the endpoint answered nothing in {} ms",
                cfg.timeout.as_millis()
            ))),
        }
    }

    /// The unbounded half of [`post_json`](Self::post_json).
    async fn send(
        &self,
        cfg: &ModelConfig,
        body: &Value,
    ) -> Result<(u16, Vec<u8>), TransportError> {
        let host = self.host.as_str();
        let port = self.port;
        let authority = format!("{host}:{port}");
        let path = self.endpoint[url::Position::BeforePath..].to_owned();
        // `Value::to_string` is the JSON text of the body, and it never fails.
        let payload = body.to_string().into_bytes();

        let mut request = Request::builder()
            .method("POST")
            .uri(&path)
            .header(HOST, &authority)
            .header(CONTENT_TYPE, "application/json")
            .header(TITLE_HEADER, TITLE);
        if !cfg.api_key.is_empty() {
            request = request.header(AUTHORIZATION, format!("Bearer {}", cfg.api_key));
        }
        let request = request
            .body(Full::new(Bytes::from(payload)))
            .map_err(|err| why("the request does not build", &err))?;

        let tcp = TcpStream::connect((host, port))
            .await
            .map_err(|err| why("the connection failed", &err))?;

        match self.tls.clone() {
            None => exchange(TokioIo::new(tcp), request).await,
            Some((config, name)) => {
                let stream = TlsConnector::from(config)
                    .connect(name, tcp)
                    .await
                    .map_err(|err| why("the TLS handshake failed", &err))?;
                exchange(TokioIo::new(stream), request).await
            }
        }
    }
}

/// Run one request/response exchange on an open stream.
async fn exchange<I>(io: I, request: Request<Full<Bytes>>) -> Result<(u16, Vec<u8>), TransportError>
where
    I: hyper::rt::Read + hyper::rt::Write + Send + Unpin + 'static,
{
    // The HTTP/1 handshake of hyper builds the sender and the connection in
    // memory. It reads no byte of the stream, so it does not fail.
    #[allow(clippy::expect_used)]
    let (mut sender, connection) = hyper::client::conn::http1::handshake(io)
        .await
        .expect("the HTTP/1 handshake of hyper builds in memory");

    // The connection future drives the socket while the request is in flight.
    // It ends when the response is read and the stream closes, so the task does
    // not outlive the call.
    tokio::spawn(async move {
        if let Err(err) = connection.await {
            tracing::debug!(error = %err, "cadus-model-client: the connection ended");
        }
    });

    let response = sender
        .send_request(request)
        .await
        .map_err(|err| why("the request failed", &err))?;
    let status = response.status().as_u16();
    let collected = Limited::new(response.into_body(), MAX_REPLY_BYTES)
        .collect()
        .await
        .map_err(|err| why("the reply did not arrive whole", &*err))?;
    Ok((status, collected.to_bytes().to_vec()))
}

/// The endpoint of `base`: [`COMPLETIONS_PATH`] after the last character of
/// `base` that is not a slash.
///
/// The text of `base` ends with its fragment, else with its query, else with
/// its path, so the path lands on that part.
fn endpoint_of(base: &Url) -> Url {
    let mut endpoint = base.clone();
    match (base.fragment(), base.query()) {
        (Some(fragment), _) => endpoint.set_fragment(Some(&with_path(fragment))),
        (None, Some(query)) => endpoint.set_query(Some(&with_path(query))),
        (None, None) => endpoint.set_path(&with_path(base.path())),
    }
    endpoint
}

/// `tail` without its trailing slashes, then [`COMPLETIONS_PATH`].
fn with_path(tail: &str) -> String {
    format!("{}{COMPLETIONS_PATH}", tail.trim_end_matches('/'))
}

/// The TLS setup: the trust anchors of `roots` over `provider`.
fn tls_config(
    provider: Arc<CryptoProvider>,
    roots: RootCertStore,
) -> Result<Arc<ClientConfig>, TransportError> {
    let config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|err| why("the TLS protocol versions did not build", &err))?
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(Arc::new(config))
}
