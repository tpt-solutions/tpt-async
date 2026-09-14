// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

#![no_main]

use libfuzzer_sys::fuzz_target;
use tpt_async_timer::wheel::TimerWheel;

/// Fuzz timer wheel insert/remove/tick: structural invariants must hold
/// under arbitrary interleavings.
fuzz_target!(|data: &[u8]| {
    let wheel: TimerWheel<8, 4> = TimerWheel::new();
    let mut ops = data.chunks(2);
    while let Some(op) = ops.next() {
        match op[0] % 3 {
            0 => wheel.tick(),
            1 => {
                let deadline = u64::from(op[1]);
                let mut entry = tpt_async_timer::wheel::TimerEntry::new(deadline);
                let ptr = core::ptr::NonNull::from(&mut entry);
                // SAFETY: entry lives for this scope and is never registered twice.
                unsafe { wheel.insert(ptr) };
            }
            _ => wheel.advance_to(u64::from(op[1])),
        }
    }
});
