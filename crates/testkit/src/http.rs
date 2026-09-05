//! The pieces of a fake HTTP/1.1 endpoint: one request read off a stream, and
//! one status reply with a JSON body.
//!
//! A fake model server is one `TcpListener` on `127.0.0.1`, a hand-written
//! reply per call, and a record of every request it received. No test reaches
//! a real provider.

use tokio::io::{AsyncRead, AsyncReadExt};

/// Read one HTTP request from `socket`: the head, then the body that
/// `content-length` names.
///
/// A stream that closes before the whole request arrived gives the bytes read
/// so far, so a hung-up client never blocks the endpoint.
pub async fn read_request<S: AsyncRead + Unpin>(socket: &mut S) -> String {
    let mut raw: Vec<u8> = Vec::new();
    let mut buffer = [0_u8; 4096];
    loop {
        let read = socket.read(&mut buffer).await.unwrap_or(0);
        if read == 0 {
            return String::from_utf8_lossy(&raw).to_string();
        }
        raw.extend_from_slice(&buffer[..read]);
        let text = String::from_utf8_lossy(&raw).to_string();
        if let Some(split) = text.find("\r\n\r\n")
            && text.len() >= split + 4 + content_length(&text[..split])
        {
            return text;
        }
    }
}

/// The `content-length` of a request head, or zero when the head names none.
fn content_length(head: &str) -> usize {
    head.to_lowercase()
        .split("\r\n")
        .find_map(|line| line.strip_prefix("content-length:"))
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(0)
}

/// The bytes of one HTTP/1.1 reply with `status` and the JSON body `payload`,
/// on a connection that closes after it.
pub fn status_reply(status: u16, payload: &str) -> String {
    format!(
        "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{payload}",
        payload.len()
    )
}

#[cfg(test)]
mod tests {
    use tokio::io::AsyncReadExt;

    use super::{read_request, status_reply};

    /// The read stops at the end of the body that `content-length` names.
    #[tokio::test]
    async fn the_read_ends_with_the_body() {
        let request = "POST /v1 HTTP/1.1\r\nContent-Length: 7\r\n\r\n{\"a\":1}";
        let mut socket: &[u8] = request.as_bytes();
        assert_eq!(read_request(&mut socket).await, request);
    }

    /// A request that arrives in three reads, the head end in the second and
    /// the body in the third, completes on the third.
    #[tokio::test]
    async fn a_request_in_three_reads_completes_on_the_third() {
        let mut socket = "POST /v1 HTTP/1.1\r\n"
            .as_bytes()
            .chain("content-length: 7\r\n\r\n".as_bytes())
            .chain("{\"a\":1}".as_bytes());
        assert_eq!(
            read_request(&mut socket).await,
            "POST /v1 HTTP/1.1\r\ncontent-length: 7\r\n\r\n{\"a\":1}"
        );
    }

    /// A head without a length has no body, so the read ends at the head.
    #[tokio::test]
    async fn a_head_without_a_length_has_no_body() {
        let request = "GET /v1 HTTP/1.1\r\nhost: x\r\n\r\n";
        let mut socket: &[u8] = request.as_bytes();
        assert_eq!(read_request(&mut socket).await, request);
    }

    /// A stream that closes early gives the bytes read so far.
    #[tokio::test]
    async fn a_closed_stream_gives_the_bytes_so_far() {
        let request = "POST /v1 HTTP/1.1\r\ncontent-length: 99\r\n\r\n{";
        let mut socket: &[u8] = request.as_bytes();
        assert_eq!(read_request(&mut socket).await, request);
    }

    #[test]
    fn the_reply_names_the_status_and_the_length() {
        assert_eq!(
            status_reply(404, "{}"),
            "HTTP/1.1 404 X\r\ncontent-type: application/json\r\ncontent-length: 2\r\nconnection: close\r\n\r\n{}"
        );
    }
}
