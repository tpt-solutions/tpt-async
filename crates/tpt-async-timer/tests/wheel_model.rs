// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Model-based check of the raw wheel: a random stream of inserts, removes,
//! ticks and jumps is mirrored by a plain `[bool]` model, and the wheel's
//! `len()` must match the model after every step.
//!
//! The same operation mix is what `fuzz/fuzz_targets/timer_wheel_tick.rs`
//! explores under libFuzzer; this copy runs on every platform in CI.

use core::ptr::NonNull;

use tpt_async_timer::wheel::{TimerEntry, TimerWheel};

const POOL: usize = 16;
const OPS: usize = 20_000;

/// Deterministic xorshift64*, so failures reproduce from the seed.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
}

fn run(seed: u64) {
    let wheel: TimerWheel<8, 4> = TimerWheel::new();
    let mut pool: [TimerEntry; POOL] = core::array::from_fn(|_| TimerEntry::new(0));
    let mut registered = [false; POOL];
    // The tick at which each entry fires: its deadline, or the next tick when
    // it was already due at insertion.
    let mut due = [0u64; POOL];
    let mut rng = Rng(seed | 1);

    for step in 0..OPS {
        let op = rng.next();
        let slot = (op >> 8) as usize % POOL;
        let arg = op >> 16;
        match op % 4 {
            0 => {
                wheel.tick();
                fire_due(&wheel, &due, &mut registered);
            }
            1 => {
                if !registered[slot] {
                    // Only unregistered entries may be reassigned; the new
                    // deadline spans several wheel levels (8, 64, 512 ticks).
                    let deadline = wheel.now() + arg % 1_500;
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
                wheel.advance_to(wheel.now() + arg % 100);
                fire_due(&wheel, &due, &mut registered);
            }
        }

        let expected = registered.iter().filter(|&&r| r).count();
        assert_eq!(
            wheel.len(),
            expected,
            "seed {seed:#x}, step {step}: wheel len disagrees with model"
        );
        assert_eq!(wheel.is_empty(), expected == 0);
    }

    // Clean up before the pool is dropped.
    for slot in 0..POOL {
        if registered[slot] {
            let entry = NonNull::from(&mut pool[slot]);
            // SAFETY: as above.
            unsafe { wheel.remove(entry) };
        }
    }
    assert!(wheel.is_empty());
}

/// Every model entry whose firing tick has been reached must have fired, so it
/// leaves the model. Advancing without reaching a tick fires nothing.
fn fire_due(wheel: &TimerWheel<8, 4>, due: &[u64; POOL], registered: &mut [bool; POOL]) {
    let now = wheel.now();
    for (slot, &at) in due.iter().enumerate() {
        if registered[slot] && at <= now {
            registered[slot] = false;
        }
    }
}

#[test]
fn random_operations_keep_len_consistent() {
    for seed in 1..=64u64 {
        run(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    }
}
