// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

#![no_main]

use core::ptr::NonNull;
use libfuzzer_sys::fuzz_target;
use tpt_async_timer::wheel::{TimerEntry, TimerWheel};

const POOL: usize = 16;

// Fuzz timer wheel insert/remove/tick/advance: the wheel's `len()` must match
// a model of which entries are still registered after every operation.
// The same model runs on every platform as `tests/wheel_model.rs`.
//
// Entries live in a fixed pool that outlives every registration: the wheel
// keeps raw pointers to registered entries, so each entry is removed before
// the pool is dropped.
fuzz_target!(|data: &[u8]| {
    let wheel: TimerWheel<8, 4> = TimerWheel::new();
    let mut pool: [TimerEntry; POOL] = core::array::from_fn(|_| TimerEntry::new(0));
    let mut registered = [false; POOL];
    // Tick at which each entry fires: its deadline, or the next tick if it was
    // already due at insertion.
    let mut due = [0u64; POOL];

    for op in data.chunks_exact(2) {
        let slot = usize::from(op[1]) % POOL;
        let arg = u64::from(op[1]);
        match op[0] % 4 {
            0 => {
                wheel.tick();
                fire_due(&wheel, &due, &mut registered);
            }
            1 => {
                if !registered[slot] {
                    let deadline = wheel.now() + arg * 7;
                    pool[slot] = TimerEntry::new(deadline);
                    due[slot] = deadline.max(wheel.now() + 1);
                    let entry = NonNull::from(&mut pool[slot]);
                    // SAFETY: the pool outlives the registration and the slot
                    // is never registered twice (guarded by `registered`).
                    unsafe { wheel.insert(entry) };
                    registered[slot] = true;
                }
            }
            2 => {
                if registered[slot] {
                    let entry = NonNull::from(&mut pool[slot]);
                    // SAFETY: `registered[slot]` means this exact entry was
                    // inserted and has not been removed or fired since.
                    unsafe { wheel.remove(entry) };
                    registered[slot] = false;
                }
            }
            _ => {
                wheel.advance_to(wheel.now() + arg);
                fire_due(&wheel, &due, &mut registered);
            }
        }

        let expected = registered.iter().filter(|&&r| r).count();
        assert_eq!(wheel.len(), expected, "len disagrees with model");
    }

    for slot in 0..POOL {
        if registered[slot] {
            let entry = NonNull::from(&mut pool[slot]);
            // SAFETY: as above; cleanup before the pool is dropped.
            unsafe { wheel.remove(entry) };
        }
    }
});

/// Entries whose firing tick has been reached have fired and leave the model.
fn fire_due(wheel: &TimerWheel<8, 4>, due: &[u64; POOL], registered: &mut [bool; POOL]) {
    let now = wheel.now();
    for (slot, &at) in due.iter().enumerate() {
        if registered[slot] && at <= now {
            registered[slot] = false;
        }
    }
}
