// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

#![no_main]

use libfuzzer_sys::fuzz_target;

/// Fuzz the zero-copy HTTP request/response head parser: any input must
/// either parse or return a structured error — never panic, never UB.
fuzz_target!(|data: &[u8]| {
    let _ = tpt_net_http::parse::fuzz_entry(data);
});
