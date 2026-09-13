// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `heartbeat` — the tpt-async timer wheel on a bare-metal Cortex-M target.
//!
//! Demonstrates the core value proposition: a heapless, allocation-free
//! timer wheel plus `Sleep` futures running on `thumbv7em-none-eabihf` with
//! **no executor, no allocator, and no OS**.  The main loop plays the part
//! of the executor: it advances the wheel from the SysTick-derived delay and
//! polls the timer futures by hand with a no-op waker.
//!
//! Build & run (hardware or QEMU):
//!
//! ```text
//! rustup target add thumbv7em-none-eabihf
//! cargo build --release --target thumbv7em-none-eabihf
//! ```
//!
//! `HEARTBEAT.count` is a `static` you can watch in a debugger; wire your
//! board's LED into `board_toggle()` to see it blink.

#![no_std]
#![no_main]

use core::future::Future;
use core::pin::pin;
use core::task::{Context, RawWaker, RawWakerVTable, Waker};

use cortex_m::Peripherals;
use cortex_m_rt::entry;
use panic_halt as _;

use tpt_async_timer::prelude::*;

/// 32 slots × 4 levels = 128 pointers on the stack/bss — about 1 KiB, zero
/// heap.  Covers 32^4 = 1,048,576 ticks (≈17 minutes at 1 kHz) before
/// wrapping.
type Wheel = TimerWheel<32, 4>;

static WHEEL: Wheel = Wheel::new();

/// Observable state for a debugger (or wire an LED here).
pub struct Heartbeat {
    pub count: u32,
}

static mut HEARTBEAT: Heartbeat = Heartbeat { count: 0 };

fn board_toggle() {
    // SAFETY: single-threaded bare metal; no reentrancy around this static.
    let hb = unsafe { &mut *core::ptr::addr_of_mut!(HEARTBEAT) };
    hb.count = hb.count.wrapping_add(1);
    // On a real board: `led.set_high()` or a GPIO BSRR write here.
}

/// A no-op waker: on bare metal the main loop polls every tick, so there is
/// nothing to wake.
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

#[entry]
fn main() -> ! {
    let cp = Peripherals::take().unwrap();
    let mut delay = cortex_m::delay::Delay::new(cp.SYST, 16_000_000); // 16 MHz

    // SysTick is configured by Delay; on a production board you would tick
    // the wheel from the SysTick interrupt handler instead.
    let waker = noop_waker();
    let mut cx = Context::from_waker(&waker);

    // One sleep future per heartbeat phase, registered against the shared
    // wheel (all wheel methods take `&self` — no exclusive borrow needed).
    let mut fast = pin!(Sleep::new(&WHEEL, WHEEL.now() + 100));
    let mut slow = pin!(Sleep::new(&WHEEL, WHEEL.now() + 500));

    loop {
        // 1 ms per loop; advance the wheel by one tick.
        delay.delay_ms(1);
        WHEEL.tick();

        // Poll every pending timer future; the superloop is the executor.
        if fast.as_mut().poll(&mut cx).is_ready() {
            board_toggle();
            // Re-arm 100 ticks from now.
            fast.as_mut().reset_pinned(WHEEL.now() + 100);
        }
        if slow.as_mut().poll(&mut cx).is_ready() {
            board_toggle();
            board_toggle();
            slow.as_mut().reset_pinned(WHEEL.now() + 500);
        }
    }
}
