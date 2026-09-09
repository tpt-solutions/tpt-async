# tpt-async

> **The runtime-agnostic async I/O facade.**

Add to your `Cargo.toml`:

```toml
[dependencies]
tpt-async    = "0.1"
tpt-net-http = "0.1"   # optional: HTTP client/server
tpt-net-tls  = "0.1"   # optional: TLS
```

Then in your code:

```rust
use tpt_async::prelude::*;
use tpt_net_http::prelude::*;

#[tpt_async::main]
async fn main() {
    let client = HttpClient::builder()
        .tls(tpt_net_tls::rustls_config())
        .build();

    let resp = client
        .get("https://api.tpt.solutions/health")
        .timeout(Duration::from_millis(500))
        .send()
        .await
        .unwrap();

    println!("Status: {}", resp.status());
}
```

## Feature flags

| Flag       | Default | Description |
|------------|---------|-------------|
| `std`      | yes     | Enables std clock, std error impls |
| `alloc`    | yes     | Enables `JoinHandle`, `Completer` |
| `executor` | yes     | Includes `LocalExecutor` and `block_on` |
| `timer`    | yes     | Includes `Sleep`, `Interval`, `Timeout` |
| `macros`   | no      | Re-exports `#[tpt_async::main]` proc-macro |

For embedded / no_std use, disable default features:

```toml
tpt-async = { version = "0.1", default-features = false, features = ["alloc"] }
```

## License

MIT OR Apache-2.0 — © 2026 TPT Solutions
