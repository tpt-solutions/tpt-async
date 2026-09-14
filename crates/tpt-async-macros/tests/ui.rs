// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Compile-fail UI tests for `#[tpt_async::main]`.
//!
//! Regenerate `.stderr` files with: `TRYBUILD=overwrite cargo test -p tpt-async-macros --test ui`

#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
