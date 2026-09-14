# tpt-async fuzz targets

Run with nightly:

```sh
cargo install cargo-fuzz
cd fuzz
cargo +nightly fuzz run http_head_parser
cargo +nightly fuzz run timer_wheel_tick
cargo +nightly fuzz run ws_frame_decode
```

Targets:

- `http_head_parser` — the zero-copy HTTP/1.1 head parser plus body-framing
  decisions (highest-value: memory-safety of range math over shared buffers)
- `timer_wheel_tick` — wheel insert/tick/advance interleavings
- `ws_frame_decode` — RFC 6455 frame decoder with role-checked masking
