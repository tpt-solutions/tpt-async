// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Autobahn testsuite echo testee.
//!
//! Pairs with the Autobahn testsuite running in fuzzing**server** mode. The
//! fuzzing server drives everything over WebSocket (`/getCaseCount`,
//! `/runCase`, `/updateReports`), so this testee speaks WebSocket on all of
//! those paths:
//!
//! ```sh
//! docker run --rm -p 9001:9001 crossbario/autobahn-testsuite \
//!     wstest --mode fuzzingserver --spec /config/fuzzingserver.json
//! ```
//!
//! with a spec that disables UTF-8 strictness overrides as needed.  This
//! client then runs every case, echoing each message back verbatim, and
//! finally asks the server to generate the report:
//!
//! ```sh
//! cargo run -p tpt-net-ws --example autobahn-echo-client -- 127.0.0.1 9001
//! ```

use tpt_async_io::TokioCompat;
use tpt_net_ws::prelude::*;

fn main() {
    let host = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1".into());
    let port: u16 = std::env::args()
        .nth(2)
        .and_then(|p| p.parse().ok())
        .unwrap_or(9001);

    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime")
        .block_on(run(&host, port));
}

async fn run(host: &str, port: u16) {
    let authority = format!("{host}:{port}");

    // 1. Ask the server how many cases exist (a single text message).
    let count = {
        let io = tokio::net::TcpStream::connect(&authority)
            .await
            .expect("tcp connect");
        let mut ws = WebSocketStream::connect(TokioCompat::new(io), "/getCaseCount", &authority)
            .await
            .expect("case count handshake");
        let count = match ws.recv().await.expect("case count message") {
            Some(Message::Text(text)) => text.trim().parse::<usize>().expect("case count"),
            other => panic!("expected case count text, got {other:?}"),
        };
        let _ = ws.close(1000).await;
        count
    };
    println!("autobahn: {count} cases");

    // 2. Run every case in echo mode.
    for case in 1..=count {
        let path = format!("/runCase?case={case}&agent=tpt-async");
        let io = tokio::net::TcpStream::connect(&authority)
            .await
            .expect("tcp connect for case");
        let mut ws = match WebSocketStream::connect(TokioCompat::new(io), &path, &authority).await {
            Ok(ws) => ws,
            Err(e) => {
                eprintln!("case {case}: handshake failed: {e}");
                continue;
            }
        };

        loop {
            match ws.recv().await {
                // Unsolicited Pongs are not echoed: the spec requires the
                // testee to send nothing back for them.
                Ok(Some(Message::Pong(_))) => continue,
                Ok(Some(message)) => {
                    if ws.send(message).await.is_err() {
                        break;
                    }
                }
                Ok(None) => break, // server closed the case
                Err(e) => {
                    eprintln!("case {case}: {e}");
                    break;
                }
            }
        }
        println!("autobahn: case {case} done");
    }

    // 3. Ask the server to regenerate its reports, then close.
    let io = tokio::net::TcpStream::connect(&authority)
        .await
        .expect("tcp connect");
    let mut ws = WebSocketStream::connect(
        TokioCompat::new(io),
        "/updateReports?agent=tpt-async",
        &authority,
    )
    .await
    .expect("update reports handshake");
    let _ = ws.close(1000).await;
    println!("autobahn: reports updated — see the fuzzingserver report page");
}
