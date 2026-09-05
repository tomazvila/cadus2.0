//! A Postgres server that finishes the handshake and then answers no query.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// The Postgres `SSLRequest` code. The client sends it before the startup
/// message, and this server declines with a single `N`.
const SSL_REQUEST_CODE: u32 = 80877103;

/// `ReadyForQuery`, transaction status `I` (idle).
const READY_FOR_QUERY: [u8; 6] = [b'Z', 0, 0, 0, 5, b'I'];

/// A server that speaks the Postgres handshake and then goes deaf.
///
/// `DeafPostgres::start` answers the TLS probe, the startup message, and the
/// pool liveness ping, so `cadus_store::connect` succeeds and the pool hands
/// out a connection. It answers nothing after the first `Parse` or `Query`
/// message, so the query never returns. That is the exact state of a database
/// that accepts a connection and then stops replying.
///
/// `DeafPostgres::start_silent` answers nothing at all, not even the TLS probe,
/// so the connect itself never returns. That is the state that `bounded`
/// bounds.
///
/// The threads end with the test process.
pub struct DeafPostgres {
    port: u16,
    query_seen: Arc<AtomicBool>,
}

impl DeafPostgres {
    /// Start a server that finishes the handshake and then goes deaf.
    pub fn start() -> DeafPostgres {
        Self::spawn(true)
    }

    /// Start a server that accepts the connection and writes nothing.
    ///
    /// The accepted streams stay open for the life of the server: a stream that
    /// goes out of scope closes the socket, and the client then reports a
    /// broken connection instead of a stall.
    pub fn start_silent() -> DeafPostgres {
        Self::spawn(false)
    }

    /// Bind a port and start the accept loop.
    fn spawn(answer: bool) -> DeafPostgres {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind the deaf server");
        let port = listener.local_addr().expect("local address").port();
        let query_seen = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&query_seen);

        std::thread::spawn(move || {
            let mut held: Vec<TcpStream> = Vec::new();
            for stream in listener.incoming().flatten() {
                if !answer {
                    held.push(stream);
                    continue;
                }
                let flag = Arc::clone(&flag);
                std::thread::spawn(move || {
                    let _ = serve_deaf(stream, &flag);
                });
            }
        });

        DeafPostgres { port, query_seen }
    }

    /// The port that this server listens on.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// A connection string that points at this server.
    pub fn dsn(&self) -> String {
        format!("postgresql://x@127.0.0.1:{}/x", self.port)
    }

    /// Report whether a query message reached the server.
    pub fn query_seen(&self) -> bool {
        self.query_seen.load(Ordering::SeqCst)
    }
}

/// Read exactly `len` bytes, or report the read error.
fn read_exact(stream: &mut TcpStream, len: usize) -> std::io::Result<Vec<u8>> {
    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf)?;
    Ok(buf)
}

/// Read the first four bytes as a big-endian unsigned number.
fn be_u32(bytes: &[u8]) -> u32 {
    u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

/// Read one packet body of the startup phase. Every packet here carries a
/// length and no type byte.
fn read_startup_packet(stream: &mut TcpStream) -> std::io::Result<Vec<u8>> {
    let header = read_exact(stream, 4)?;
    read_exact(stream, be_u32(&header) as usize - 4)
}

/// (a) The startup phase: decline TLS with `N` and take the next packet as the
/// startup message.
///
/// A `TcpStream` write goes to the socket at once, so no write here needs a
/// flush.
fn handshake(stream: &mut TcpStream) -> std::io::Result<()> {
    loop {
        let body = read_startup_packet(stream)?;
        if body.len() < 4 || be_u32(&body) != SSL_REQUEST_CODE {
            return Ok(());
        }
        stream.write_all(b"N")?;
    }
}

/// The bytes of a finished start-up: AuthenticationOk, one ParameterStatus,
/// BackendKeyData, ReadyForQuery.
fn startup_reply() -> Vec<u8> {
    let payload = b"server_version\x0016.0\x00";
    let mut reply = vec![b'R', 0, 0, 0, 8, 0, 0, 0, 0, b'S'];
    reply.extend_from_slice(&(payload.len() as u32 + 4).to_be_bytes());
    reply.extend_from_slice(payload);
    reply.extend_from_slice(&[b'K', 0, 0, 0, 12, 0, 0, 0, 1, 0, 0, 0, 1]);
    reply.extend_from_slice(&READY_FOR_QUERY);
    reply
}

/// (b) Report a finished start-up in one write.
fn announce_ready(stream: &mut TcpStream) -> std::io::Result<()> {
    stream.write_all(&startup_reply())
}

/// (c) Answer the pool liveness ping (a bare `Sync`), then go deaf on the
/// first real query. Every packet here carries a type byte and a length.
fn answer_until_deaf(stream: &mut TcpStream, query_seen: &AtomicBool) -> std::io::Result<()> {
    let mut deaf = false;
    loop {
        let kind = read_exact(stream, 1)?[0];
        let header = read_exact(stream, 4)?;
        let _body = read_exact(stream, be_u32(&header) as usize - 4)?;
        match kind {
            b'X' => return Ok(()),
            b'P' | b'Q' => {
                deaf = true;
                query_seen.store(true, Ordering::SeqCst);
            }
            b'S' if !deaf => stream.write_all(&READY_FOR_QUERY)?,
            _ => {}
        }
    }
}

/// Serve one connection: finish the handshake, answer the ping, go deaf.
fn serve_deaf(mut stream: TcpStream, query_seen: &AtomicBool) -> std::io::Result<()> {
    handshake(&mut stream)?;
    announce_ready(&mut stream)?;
    answer_until_deaf(&mut stream, query_seen)
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::net::{Shutdown, TcpListener, TcpStream};
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::{READY_FOR_QUERY, answer_until_deaf, handshake, serve_deaf, startup_reply};

    /// The `SSLRequest` packet: length 8, code 80877103.
    const SSL_REQUEST: [u8; 8] = [0, 0, 0, 8, 4, 210, 22, 47];

    /// A startup message with no parameter: length 8, protocol 3.0.
    const STARTUP: [u8; 8] = [0, 0, 0, 8, 0, 3, 0, 0];

    /// A bare `Sync`: the liveness ping of the pool.
    const SYNC: [u8; 5] = [b'S', 0, 0, 0, 4];

    /// One connected pair on the loopback: the client end and the server end.
    fn pair() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (server, _) = listener.accept().unwrap();
        (client, server)
    }

    /// The server end of a pair whose client wrote `bytes` and closed.
    fn server_after(bytes: &[&[u8]]) -> TcpStream {
        let (mut client, server) = pair();
        for part in bytes {
            client.write_all(part).unwrap();
        }
        client.shutdown(Shutdown::Both).unwrap();
        drop(client);
        server
    }

    /// The start-up reply opens with AuthenticationOk and ends with
    /// ReadyForQuery.
    #[test]
    fn the_startup_reply_runs_from_authentication_ok_to_ready_for_query() {
        let reply = startup_reply();
        assert_eq!(&reply[..9], &[b'R', 0, 0, 0, 8, 0, 0, 0, 0]);
        assert!(reply.ends_with(&READY_FOR_QUERY));
    }

    /// A client that closes early is a read error at the read that meets the
    /// end: the startup header, the startup body, the message kind, the
    /// message header, and the message body.
    #[test]
    fn a_client_that_closes_early_is_a_read_error() {
        let seen = AtomicBool::new(false);
        let closed_in_startup: [&[&[u8]]; 2] = [&[], &[&[0, 0, 0, 12]]];
        for bytes in closed_in_startup {
            assert!(serve_deaf(server_after(bytes), &seen).is_err());
        }
        let closed_in_answer: [&[&[u8]]; 3] = [
            &[&STARTUP],
            &[&STARTUP, b"S"],
            &[&STARTUP, &[b'S', 0, 0, 0, 8]],
        ];
        for bytes in closed_in_answer {
            assert!(serve_deaf(server_after(bytes), &seen).is_err());
        }
        assert!(!seen.load(std::sync::atomic::Ordering::SeqCst));
    }

    /// A server end whose write side is shut reports the error at the write:
    /// the `N` of the TLS probe, the start-up reply, and the answer to the
    /// liveness ping.
    #[test]
    fn a_shut_write_side_is_a_write_error() {
        let seen = AtomicBool::new(false);

        let (mut client, mut server) = pair();
        client.write_all(&SSL_REQUEST).unwrap();
        server.shutdown(Shutdown::Write).unwrap();
        assert!(handshake(&mut server).is_err());

        let (mut client, server) = pair();
        client.write_all(&STARTUP).unwrap();
        server.shutdown(Shutdown::Write).unwrap();
        assert!(serve_deaf(server, &seen).is_err());

        let (mut client, mut server) = pair();
        client.write_all(&SYNC).unwrap();
        server.shutdown(Shutdown::Write).unwrap();
        assert!(answer_until_deaf(&mut server, &seen).is_err());
    }

    /// A finished handshake answers the liveness ping, goes deaf on the first
    /// query, ignores a second ping and an unknown message, and returns on the
    /// terminate.
    #[test]
    fn a_finished_handshake_answers_the_ping_then_goes_deaf() {
        let seen = AtomicBool::new(false);
        let (mut client, mut server) = pair();
        let parts: [&[u8]; 5] = [
            &SYNC,
            &[b'P', 0, 0, 0, 4],
            &SYNC,
            &[b'D', 0, 0, 0, 4],
            &[b'X', 0, 0, 0, 4],
        ];
        for part in parts {
            client.write_all(part).unwrap();
        }
        assert!(answer_until_deaf(&mut server, &seen).is_ok());
        assert!(seen.load(Ordering::SeqCst));
        drop(client);
    }
}
