// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Integration test: `TokioReader`/`TokioWriter` adapters + ext traits, with
//! a real tokio in-memory pipe underneath.  Exercises poll_read/poll_write/
//! poll_flush/poll_shutdown and the `AsyncReadExt`/`AsyncWriteExt` helpers
//! end to end.

use std::pin::Pin;

use tpt_async_io::{AsyncReadExt, AsyncWriteExt, ReadBuf, TokioReader, TokioWriter};

#[tokio::test]
async fn tokio_adapter_roundtrip_with_ext_traits() {
    let (client, server) = tokio::io::duplex(64 * 1024);

    // Writer side: plaintext through the TokioWriter adapter.
    let writer_task = tokio::spawn(async move {
        let mut writer = TokioWriter::new(client);
        writer.write_all(b"hello tpt-async").await?;
        writer.flush().await?;
        writer.shutdown().await?;
        Ok::<(), tpt_async_io::IoError>(())
    });

    // Reader side: chunked reads through the TokioReader adapter.
    let mut reader = TokioReader::new(server);
    let mut received = Vec::new();
    let mut buf = [0u8; 8]; // small on purpose: several partial reads

    loop {
        let n = reader.read(ReadBuf::new(&mut buf)).await.unwrap();
        if n == 0 {
            break; // shutdown() produced EOF
        }
        received.extend_from_slice(&buf[..n]);
        if received == b"hello tpt-async" {
            break;
        }
    }

    assert_eq!(received, b"hello tpt-async");
    writer_task.await.unwrap().unwrap();
}

#[tokio::test]
async fn std_adapter_roundtrip_with_cursor() {
    use std::io::Cursor;
    use std::pin::pin;
    use tpt_async_io::{AsyncRead, StdReader};

    // Raw-trait path: read a Cursor in 4-byte chunks until EOF.
    let mut reader = StdReader::new(Cursor::new(b"payload".to_vec()));
    let mut buf = [0u8; 4];
    let mut chunks = 0;
    let mut raw_total = 0;
    let mut raw_fut = pin!(async {
        loop {
            let mut read_buf = ReadBuf::new(&mut buf);
            std::future::poll_fn(|cx| Pin::new(&mut reader).poll_read(cx, &mut read_buf))
                .await
                .unwrap();
            let n = read_buf.filled().len();
            if n == 0 {
                break; // EOF
            }
            raw_total += n;
            chunks += 1;
        }
    });
    raw_fut.await;
    assert_eq!(raw_total, 7);
    assert!(chunks >= 2);

    // Ext-trait path: read_to_end collects everything.
    let mut reader2 = StdReader::new(Cursor::new(b"payload".to_vec()));
    let mut out = Vec::new();
    reader2.read_to_end(&mut out).await.unwrap();
    assert_eq!(out, b"payload");
}

#[async_std::test]
async fn async_std_writer_flushes_to_buffer() {
    use tpt_async_io::AsyncStdWriter;

    // Writer path: async-std BufWriter over a Vec (which implements the
    // futures-io AsyncWrite that async-std blankets over) — write_all then
    // flush must land every byte in the wrapped buffer.
    let mut writer = AsyncStdWriter::new(async_std::io::BufWriter::new(Vec::new()));
    writer
        .write_all(b"async-std pairing")
        .await
        .expect("write_all");
    writer.flush().await.expect("flush");
    let written = writer.into_inner().into_inner().await.expect("into_inner");
    assert_eq!(written, b"async-std pairing");
}
