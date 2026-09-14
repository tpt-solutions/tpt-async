// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

#![no_main]

use libfuzzer_sys::fuzz_target;
use tpt_net_ws::frame::{decode, Role};

/// Fuzz the WS frame decoder: malformed input must yield structured errors,
/// never panics or over-reads.
fuzz_target!(|data: &[u8]| {
    let _ = decode(data, Role::Server);
    let _ = decode(data, Role::Client);
});
