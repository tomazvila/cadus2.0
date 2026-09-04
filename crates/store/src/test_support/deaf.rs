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
fn handshake(stream: &mut TcpStream) -> std::io::Result<()> {
    loop {
        let body = read_startup_packet(stream)?;
        if body.len() < 4 || be_u32(&body) != SSL_REQUEST_CODE {
            return Ok(());
        }
        stream.write_all(b"N")?;
        stream.flush()?;
    }
}

/// (b) Report a finished start-up: AuthenticationOk, one ParameterStatus,
/// BackendKeyData, ReadyForQuery.
fn announce_ready(stream: &mut TcpStream) -> std::io::Result<()> {
    stream.write_all(&[b'R', 0, 0, 0, 8, 0, 0, 0, 0])?;
    let payload = b"server_version\x0016.0\x00";
    let mut status = vec![b'S'];
    status.extend_from_slice(&(payload.len() as u32 + 4).to_be_bytes());
    status.extend_from_slice(payload);
    stream.write_all(&status)?;
    stream.write_all(&[b'K', 0, 0, 0, 12, 0, 0, 0, 1, 0, 0, 0, 1])?;
    stream.write_all(&READY_FOR_QUERY)?;
    stream.flush()
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
            b'S' if !deaf => {
                stream.write_all(&READY_FOR_QUERY)?;
                stream.flush()?;
            }
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
