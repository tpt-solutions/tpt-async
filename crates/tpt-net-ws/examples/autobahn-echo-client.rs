// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Autobahn testsuite echo client (fuzzingclient mode).
//!
//! Pairs with the Autobahn testsuite running in fuzzing**server** mode:
//!
//! ```sh
//! docker run --rm -p 9001:9001 crossbario/autobahntestsuite \
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

use std::time::Duration;

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

    // 1. Ask the server how many cases exist.
    let count = {
        let io = tokio::net::TcpStream::connect(&authority)
            .await
            .expect("tcp connect");
        let mut conn = tpt_net_http::ClientConnection::new(TokioCompat::new(io));
        let mut resp = conn
            .send(&tpt_net_http::Request::new("GET", "/getCaseCount"), host)
            .await
            .expect("case count request");
        let body = resp.body_bytes(64).await.expect("case count body");
        String::from_utf8_lossy(&body)
            .trim()
            .parse::<usize>()
            .expect("case count")
    };
    println!("autobahn: {count} cases");

    // 2. Run every case in echo mode.
    for case in 1..=count {
        let path = format!("/runCase?case={case}&agent=tpt-async");
        let io = tokio::net::TcpStream::connect(&authority)
            .await
            .expect("tcp connect for case");
        let mut ws = match WebSocketStream::connect(TokioCompat::new(io), &path, host).await {
            Ok(ws) => ws,
            Err(e) => {
                eprintln!("case {case}: handshake failed: {e}");
                continue;
            }
        };

        loop {
            match ws.recv().await {
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

    // 3. Ask for the report.
    let io = tokio::net::TcpStream::connect(&authority)
        .await
        .expect("tcp connect");
    let mut conn = tpt_net_http::ClientConnection::new(TokioCompat::new(io));
    let mut resp = conn
        .send(
            &tpt_net_http::Request::new("GET", &format!("/updateReports?agent=tpt-async")),
            host,
        )
        .await
        .expect("update reports");
    let _ = resp.body_bytes(4096).await;
    println!("autobahn: reports updated — see the fuzzingserver report page");
}
