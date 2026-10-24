// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

#![no_main]

use core::ptr::NonNull;
use libfuzzer_sys::fuzz_target;
use tpt_async_timer::wheel::{TimerEntry, TimerWheel};

const POOL: usize = 16;

// Fuzz timer wheel insert/remove/tick: structural invariants must hold
// under arbitrary interleavings.
//
// Entries live in a fixed pool that outlives every registration: the wheel
// keeps raw pointers to registered entries, so each entry is removed before
// the pool is dropped.
fuzz_target!(|data: &[u8]| {
    let wheel: TimerWheel<8, 4> = TimerWheel::new();
    let mut pool: [TimerEntry; POOL] = core::array::from_fn(|_| TimerEntry::new(0));
    let mut registered = [false; POOL];

    for op in data.chunks_exact(2) {
        let slot = usize::from(op[1]) % POOL;
        match op[0] % 4 {
            0 => wheel.tick(),
            1 => {
                if !registered[slot] {
                    let entry = NonNull::from(&mut pool[slot]);
                    // SAFETY: the pool outlives the registration and the
                    // slot is never registered twice (guarded by `registered`).
                    unsafe { wheel.insert(entry) };
                    registered[slot] = true;
                }
            }
            2 => {
                if registered[slot] {
                    let entry = NonNull::from(&mut pool[slot]);
                    // SAFETY: `registered[slot]` means this exact entry was
                    // inserted and has not been removed since.
                    unsafe { wheel.remove(entry) };
                    registered[slot] = false;
                }
            }
            _ => wheel.advance_to(u64::from(op[1])),
        }
    }

    for slot in 0..POOL {
        if registered[slot] {
            let entry = NonNull::from(&mut pool[slot]);
            // SAFETY: as above; cleanup before the pool is dropped.
            unsafe { wheel.remove(entry) };
        }
    }
});
