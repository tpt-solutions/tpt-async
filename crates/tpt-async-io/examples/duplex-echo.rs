// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Echo over an in-memory duplex using the ext helpers, proving that
//! protocol code only needs the `tpt-async-io` traits.

use tpt_async_io::prelude::*;
use tpt_async_io::{TokioReader, TokioWriter};

#[tokio::main]
async fn main() {
    let (writer_half, reader_half) = tokio::io::duplex(4096);

    // Producer side:
    tokio::spawn(async move {
        let mut writer = TokioWriter::new(writer_half);
        writer
            .write_all(b"hello from tpt-async-io")
            .await
            .expect("write");
        writer.flush().await.expect("flush");
        writer.shutdown().await.expect("shutdown");
    });

    // Consumer side: read to EOF through the adapter.
    let mut reader = TokioReader::new(reader_half);
    let mut out = Vec::new();
    reader.read_to_end(&mut out).await.expect("read_to_end");

    println!("io example: received {:?}", String::from_utf8_lossy(&out));
}
