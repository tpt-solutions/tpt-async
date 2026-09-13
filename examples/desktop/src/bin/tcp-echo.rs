//! `tcp-echo` — tokio runtime + `tpt-async` traits, showing interop.
//!
//! tokio provides the I/O driver (readiness wakes for sockets); the echo
//! protocol code is written entirely against `tpt-async-io` traits and uses
//! the `tpt-async` timer and `Spawn` adapter — swap the runtime out and the
//! application code stays unchanged.
//!
//! ```text
//! cargo run -p desktop-examples --bin tcp-echo
//! # in another terminal:
//! nc localhost 8080
//! ```

use std::time::Duration;

use tpt_async::prelude::*;
use tpt_async_io::{ReadBuf, TokioReader, TokioWriter};

const READ_CHUNK: usize = 4096;
const READ_TIMEOUT: Duration = Duration::from_secs(30);

fn main() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    rt.block_on(async {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:8080")
            .await
            .expect("bind 127.0.0.1:8080");
        println!("tcp-echo: listening on 127.0.0.1:8080 (Ctrl-C to quit)");

        // The unified Spawn trait works on tokio's runtime handle.
        let spawner =
            tpt_async::runtime::tokio_support::TokioHandle(tokio::runtime::Handle::current());

        loop {
            let (socket, peer) = listener.accept().await.expect("accept");
            println!("tcp-echo: connection from {peer}");

            socket.set_nodelay(true).ok();

            spawner
                .spawn(async move {
                    if let Err(e) = echo(socket).await {
                        eprintln!("tcp-echo: connection {peer} ended: {e}");
                    }
                })
                .expect("spawn");
        }
    });
}

/// Echo every line back, with an idle timeout — all through `tpt-async` traits.
async fn echo(socket: tokio::net::TcpStream) -> Result<(), tpt_async_io::IoError> {
    // Wrap the tokio socket in the tpt-async-io adapters: everything below
    // speaks only tpt_async_io::AsyncRead/AsyncWrite.
    let (reader, writer) = socket.into_split();
    let mut reader = TokioReader::new(reader);
    let mut writer = TokioWriter::new(writer);

    let mut buf = [0u8; READ_CHUNK];
    let mut total = 0u64;

    loop {
        // Idle connections are closed by the heapless-timer-backed timeout.
        // (Own function so borrows end before the echo write below.)
        let Some(n) = read_with_timeout(&mut reader, &mut buf).await? else {
            println!("tcp-echo: idle timeout");
            return Ok(());
        };

        if n == 0 {
            return Ok(()); // peer closed
        }

        writer.write_all(&buf[..n]).await?;
        writer.flush().await?;
        total += n as u64;
        println!("tcp-echo: echoed {total} bytes so far");
    }
}

/// One read with an idle timeout.  `Ok(None)` = timed out; `Ok(Some(0))` =
/// peer closed.
async fn read_with_timeout(
    reader: &mut TokioReader<tokio::net::tcp::OwnedReadHalf>,
    out: &mut [u8],
) -> Result<Option<usize>, tpt_async_io::IoError> {
    let read_fut = async { reader.read(ReadBuf::new(out)).await };
    match tpt_async::timeout(READ_TIMEOUT, read_fut).await {
        Ok(result) => result.map(Some),
        Err(_timed_out) => Ok(None),
    }
}
