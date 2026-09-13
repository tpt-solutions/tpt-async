// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `wasm-example` — the tpt-async timer wheel in the browser.
//!
//! There is no `std` thread on `wasm32-unknown-unknown`, so the timer
//! crate's background driver is unavailable; instead this example wires the
//! wheel to `performance.now()` (via `js_sys`) and exposes a
//! `requestAnimationFrame`-free polling API the page can call from its own
//! rAF loop.
//!
//! Build:
//!
//! ```text
//! rustup target add wasm32-unknown-unknown
//! cargo build --target wasm32-unknown-unknown
//! ```
//!
//! Then import `pkg/wasm_example.js` from a bundler, or serve with
//! `wasm-pack serve` after `wasm-pack build`.

use core::future::Future;
use core::pin::pin;
use core::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

use wasm_bindgen::prelude::*;

use tpt_async_timer::prelude::*;

/// 64 slots × 4 levels at 1 ms resolution ≈ 4.3 billion ticks of range.
type Wheel = TimerWheel<64, 4>;

static WHEEL: Wheel = Wheel::new();

thread_local! {
    static PAGE_START_MS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Milliseconds since `init_clock` was called, from `performance.now()`.
pub fn now_ms() -> u64 {
    let page_start = PAGE_START_MS.with(|s| s.get());
    (js_sys::Date::now() as u64).saturating_sub(page_start)
}


fn noop_waker() -> Waker {
    static VTABLE: RawWakerVTable = RawWakerVTable::new(
        |ptr| RawWaker::new(ptr, &VTABLE),
        |_| {},
        |_| {},
        |_| {},
    );
    // SAFETY: the vtable ignores the data pointer.
    unsafe { Waker::from_raw(RawWaker::new(core::ptr::null(), &VTABLE)) }
}

/// A future that resolves `deadline_ms` after `start_timer` was called.
struct PageSleep {
    deadline_ms: u64,
}

impl Future for PageSleep {
    type Output = ();

    fn poll(self: core::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if now_ms() >= self.deadline_ms {
            Poll::Ready(())
        } else {
            // In a real page you would register a waker that the rAF loop
            // invokes; here the page drives polling explicitly.
            let _ = cx.waker();
            Poll::Pending
        }
    }
}

/// Initialize the clock baseline.  Call once from JS at startup.
#[wasm_bindgen]
pub fn init_clock() {
    PAGE_START_MS.with(|s| s.set(js_sys::Date::now() as u64));
}

/// Returns `true` once `ms` milliseconds have passed since `start_timer`.
/// The page's rAF loop keeps calling this; the wheel advances with it.
#[wasm_bindgen]
pub fn poll_timer(ms: u32) -> bool {
    // Advance the wheel to wall-clock time; this wakes any due entries.
    WHEEL.advance_to(now_ms());

    let deadline = now_ms() + ms as u64;
    let mut fut = pin!(PageSleep {
        deadline_ms: deadline,
    });
    let waker = noop_waker();
    let mut cx = Context::from_waker(&waker);
    fut.as_mut().poll(&mut cx).is_ready()
}
