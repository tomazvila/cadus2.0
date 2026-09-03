//! The fake OpenAI-compatible server of the client tests, and the fixtures
//! every test sends.
//!
//! The endpoint is one `TcpListener` on `127.0.0.1`, a hand-written HTTP/1.1
//! reply per call, and a record of every request it received. No test reaches
//! a real provider. Every test binary compiles this module, and no binary uses
//! every helper, so the dead-code lint is off for the module.

#![allow(dead_code)]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use cadus_model_client::{Call, ChatRequest, Client, ModelConfig, ToolSpec};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

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
async fn read_request(socket: &mut TcpStream) -> String {
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
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let record = Arc::clone(&seen);

        tokio::spawn(async move {
            let mut index = 0_usize;
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                let request = read_request(&mut socket).await;
                let split = request.find("\r\n\r\n").unwrap_or(request.len());
                let head = request[..split].to_lowercase();
                let body: Value = serde_json::from_str(request.get(split + 4..).unwrap_or(""))
                    .unwrap_or(Value::Null);
                record.lock().unwrap().push(Seen { head, body });

                let reply = replies
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| Reply::Status(500, String::new()));
                index += 1;
                if let Some(bytes) = reply_bytes(reply) {
                    let _ = socket.write_all(bytes.as_bytes()).await;
                    let _ = socket.flush().await;
                }
            }
        });

        FakeModel {
            base_url: format!("http://127.0.0.1:{port}/v1"),
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
