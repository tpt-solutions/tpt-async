//! Hierarchical timer wheel — O(1) insert and remove.
//!
//! # Level layout
//!
//! - Level 0 resolution: 1 tick; covers `0..SLOTS` ticks ahead
//! - Level k resolution: `SLOTS^k` ticks; covers `0..SLOTS^(k+1)` ticks ahead
//!
//! # Invariants
//! - Entries must be `Pin`ned for their entire registration lifetime.
//! - Entries must be removed before being dropped.

use core::ptr::NonNull;
use core::task::Waker;

// ── TimerEntry ────────────────────────────────────────────────────────────────

/// An intrusive linked-list node stored inline inside a [`Sleep`](crate::sleep::Sleep) future.
///
/// # Safety
/// An entry must not be moved or dropped while it is registered in a wheel.
pub struct TimerEntry {
    /// Absolute tick at which this entry fires.
    pub(crate) deadline: u64,
    /// Waker to call when the entry fires.
    pub(crate) waker: Option<Waker>,
    /// Next entry in the same slot's intrusive linked list.
    pub(crate) next: Option<NonNull<TimerEntry>>,
    /// Points to the `next` field that points to *this* entry (either the slot
    /// head pointer or a predecessor's `next` field).  Allows O(1) removal.
    pub(crate) prev_next: Option<NonNull<Option<NonNull<TimerEntry>>>>,
}

impl TimerEntry {
    /// Create a new, unregistered entry with the given deadline.
    #[must_use]
    pub const fn new(deadline: u64) -> Self {
        Self {
            deadline,
            waker: None,
            next: None,
            prev_next: None,
        }
    }

    /// Returns `true` if this entry is currently registered in a wheel.
    #[inline]
    pub fn is_registered(&self) -> bool {
        self.prev_next.is_some()
    }
}

// ── TimerWheel ────────────────────────────────────────────────────────────────

/// A hierarchical timer wheel with `LEVELS` levels each having `SLOTS` slots.
///
/// Type parameters
/// - `SLOTS`: number of slots per level.  Must be ≥ 2.
/// - `LEVELS`: number of levels.  Must be ≥ 1.
///
/// The wheel supports O(1) insert, O(1) remove, and amortised O(1) `tick`.
pub struct TimerWheel<const SLOTS: usize, const LEVELS: usize> {
    /// `heads[level][slot]` — head of the intrusive singly-linked list.
    heads: [[Option<NonNull<TimerEntry>>; SLOTS]; LEVELS],
    current_tick: u64,
}

// SAFETY: `TimerWheel` only holds raw pointers to `TimerEntry` nodes that are
// owned and pinned by callers.  As long as callers uphold the documented safety
// contract the wheel is safe to send across threads.
unsafe impl<const SLOTS: usize, const LEVELS: usize> Send
    for TimerWheel<SLOTS, LEVELS>
{
}

impl<const SLOTS: usize, const LEVELS: usize> TimerWheel<SLOTS, LEVELS> {
    /// Create a new, empty timer wheel.
    pub const fn new() -> Self {
        // `Option<NonNull<…>>` is `None` when zero-initialised.
        // We cannot use `[None; SLOTS]` in a const context without `Copy`
        // on `Option<NonNull<…>>` (which is stable), so we use a transmute.
        // SAFETY: `Option<NonNull<TimerEntry>>` has the same layout as a
        // pointer-sized value where 0 == None; this is guaranteed by the null
        // pointer optimisation.
        Self {
            heads: [[None; SLOTS]; LEVELS],
            current_tick: 0,
        }
    }

    /// The current tick count.
    #[inline]
    pub fn now(&self) -> u64 {
        self.current_tick
    }

    // ── slot selection ────────────────────────────────────────────────────

    /// Choose `(level, slot)` for an entry with the given `deadline`.
    fn choose_slot(&self, deadline: u64) -> (usize, usize) {
        let delta = deadline.saturating_sub(self.current_tick);
        if delta == 0 {
            // Fire immediately by putting in the *current* level-0 slot;
            // `tick()` will drain it before incrementing.
            let slot = (self.current_tick % SLOTS as u64) as usize;
            return (0, slot);
        }

        // Find floor(log_SLOTS(delta)), clamped to LEVELS-1.
        let mut level = 0usize;
        let mut width = SLOTS as u64; // SLOTS^(level+1)
        while level + 1 < LEVELS && delta >= width {
            level += 1;
            width = width.saturating_mul(SLOTS as u64);
        }

        // Slot index within that level.
        // Level k has resolution SLOTS^k ticks.
        let resolution = (SLOTS as u64).pow(level as u32);
        let slot = ((deadline / resolution) % SLOTS as u64) as usize;
        (level, slot)
    }

    // ── public API ────────────────────────────────────────────────────────

    /// Insert an already-pinned `TimerEntry` into the wheel.
    ///
    /// # Safety
    /// - `entry` must be valid, pinned, and not currently registered in any wheel.
    /// - `entry` must remain pinned and valid until it is removed (or the wheel
    ///   fires it).
    pub unsafe fn insert(&mut self, mut entry: NonNull<TimerEntry>) {
        // SAFETY: caller guarantees the pointer is valid and pinned.
        let e = unsafe { entry.as_mut() };
        debug_assert!(
            !e.is_registered(),
            "TimerEntry inserted while already registered"
        );

        let (level, slot) = self.choose_slot(e.deadline);

        // Link at the front of heads[level][slot].
        let head_ptr: *mut Option<NonNull<TimerEntry>> =
            &mut self.heads[level][slot];

        e.next = self.heads[level][slot];
        // SAFETY: `head_ptr` points into `self.heads` which is valid for the
        // lifetime of `self`.
        e.prev_next = Some(unsafe { NonNull::new_unchecked(head_ptr) });

        // If there was a previous head, update its prev_next.
        if let Some(mut old_head) = e.next {
            // SAFETY: old_head is a registered entry and thus valid.
            unsafe { old_head.as_mut() }.prev_next =
                Some(unsafe { NonNull::new_unchecked(&mut e.next as *mut _) });
        }

        self.heads[level][slot] = Some(entry);
    }

    /// Remove a `TimerEntry` from the wheel.
    ///
    /// Safe to call even if the entry is not currently registered (no-op).
    ///
    /// # Safety
    /// - `entry` must still be pinned and valid.
    pub unsafe fn remove(&mut self, mut entry: NonNull<TimerEntry>) {
        // SAFETY: caller guarantees the pointer is valid.
        let e = unsafe { entry.as_mut() };

        let prev_next_ptr = match e.prev_next.take() {
            Some(p) => p,
            None => return, // not registered
        };

        // Write our `next` into the slot that was pointing at us.
        // SAFETY: prev_next_ptr is a valid pointer to an `Option<NonNull<TimerEntry>>`
        // field (either in a slot head or in a predecessor entry's `next`).
        unsafe { *prev_next_ptr.as_ptr() = e.next };

        // Update the successor's prev_next to skip over us.
        if let Some(mut next_entry) = e.next.take() {
            // SAFETY: next_entry is registered and thus valid.
            unsafe { next_entry.as_mut() }.prev_next = Some(prev_next_ptr);
        }
    }

    /// Advance the wheel by one tick, waking all entries whose deadline has passed.
    pub fn tick(&mut self) {
        // Drain the current slot on level 0.
        let slot0 = (self.current_tick % SLOTS as u64) as usize;
        self.drain_slot(0, slot0);

        self.current_tick += 1;

        // Cascade higher levels when a level-0 wrap occurs.
        // Level k cascades when current_tick is a multiple of SLOTS^(k).
        let mut width = SLOTS as u64;
        for level in 1..LEVELS {
            if self.current_tick % width == 0 {
                let slot = ((self.current_tick / width)
                    % SLOTS as u64) as usize;
                self.cascade(level, slot);
            } else {
                break;
            }
            width = width.saturating_mul(SLOTS as u64);
        }
    }

    // ── private helpers ───────────────────────────────────────────────────

    /// Drain a slot: wake every entry in it.
    fn drain_slot(&mut self, level: usize, slot: usize) {
        // Detach the whole list.
        let mut cursor = self.heads[level][slot].take();
        while let Some(mut node_ptr) = cursor {
            // SAFETY: all registered entries are valid and pinned.
            let node = unsafe { node_ptr.as_mut() };
            cursor = node.next.take();
            node.prev_next = None; // mark as unregistered

            if let Some(waker) = node.waker.take() {
                waker.wake();
            }
        }
    }

    /// Take all entries from `heads[level][slot]` and re-insert them at a
    /// lower level (they now fall within the finer-grained range).
    fn cascade(&mut self, level: usize, slot: usize) {
        // Detach the list.
        let mut cursor = self.heads[level][slot].take();
        while let Some(mut node_ptr) = cursor {
            // SAFETY: registered entries are valid and pinned.
            let node = unsafe { node_ptr.as_mut() };
            cursor = node.next.take();
            node.prev_next = None; // temporarily unregistered

            // Re-insert at the correct (lower) level.
            // SAFETY: node is still pinned; we just cleared its registration
            // state so insert preconditions hold.
            unsafe { self.insert(node_ptr) };
        }
    }
}

impl<const SLOTS: usize, const LEVELS: usize> Default
    for TimerWheel<SLOTS, LEVELS>
{
    fn default() -> Self {
        Self::new()
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use core::pin::Pin;
    use core::sync::atomic::{AtomicU32, Ordering};

    // 8 slots, 4 levels — compact wheel for tests.
    type Wheel = TimerWheel<8, 4>;

    /// Allocate a `TimerEntry` on the stack, pin it, and run `f` with a
    /// `NonNull` pointer to it.
    ///
    /// SAFETY: the closure must not move or drop the entry while a pointer to
    /// it is held.
    macro_rules! with_entry {
        ($deadline:expr, $name:ident, $body:block) => {
            let mut _storage = TimerEntry::new($deadline);
            // SAFETY: we never move _storage while $name is live.
            let $name: NonNull<TimerEntry> =
                unsafe { NonNull::new_unchecked(&mut _storage as *mut _) };
            $body
        };
    }

    // Build a Waker that increments a static AtomicU32 on wake.
    fn make_waker(counter: &'static AtomicU32) -> Waker {
        use core::task::{RawWaker, RawWakerVTable};

        static VTABLE: RawWakerVTable = RawWakerVTable::new(
            // clone
            |ptr| RawWaker::new(ptr, &VTABLE),
            // wake (consumes)
            |ptr| {
                // SAFETY: ptr is a valid `*const AtomicU32` cast.
                unsafe { &*(ptr as *const AtomicU32) }
                    .fetch_add(1, Ordering::SeqCst);
            },
            // wake_by_ref
            |ptr| {
                // SAFETY: same as above.
                unsafe { &*(ptr as *const AtomicU32) }
                    .fetch_add(1, Ordering::SeqCst);
            },
            // drop
            |_| {},
        );

        // SAFETY: `counter` is a `'static` reference so the pointer is always valid.
        let ptr = counter as *const AtomicU32 as *const ();
        unsafe { Waker::from_raw(RawWaker::new(ptr, &VTABLE)) }
    }

    #[test]
    fn insert_and_tick_fires() {
        let mut wheel = Wheel::new();
        static WOKEN: AtomicU32 = AtomicU32::new(0);

        with_entry!(3u64, entry_ptr, {
            unsafe { entry_ptr.as_mut() }.waker = Some(make_waker(&WOKEN));
            // SAFETY: entry is pinned on the stack and not registered.
            unsafe { wheel.insert(entry_ptr) };

            wheel.tick(); // tick 0→1
            assert_eq!(WOKEN.load(Ordering::SeqCst), 0, "fired too early at tick 1");
            wheel.tick(); // tick 1→2
            assert_eq!(WOKEN.load(Ordering::SeqCst), 0, "fired too early at tick 2");
            wheel.tick(); // tick 2→3  — drains slot for tick==3
            assert_eq!(WOKEN.load(Ordering::SeqCst), 1, "not fired after tick 3");
        });
    }

    #[test]
    fn early_tick_does_not_fire() {
        let mut wheel = Wheel::new();
        static WOKEN: AtomicU32 = AtomicU32::new(0);

        with_entry!(5u64, entry_ptr, {
            unsafe { entry_ptr.as_mut() }.waker = Some(make_waker(&WOKEN));
            unsafe { wheel.insert(entry_ptr) };

            wheel.tick();
            wheel.tick();
            assert_eq!(WOKEN.load(Ordering::SeqCst), 0);

            // Clean up — remove before the entry is dropped.
            unsafe { wheel.remove(entry_ptr) };
        });
    }

    #[test]
    fn multiple_entries_same_slot() {
        let mut wheel = Wheel::new();
        static W1: AtomicU32 = AtomicU32::new(0);
        static W2: AtomicU32 = AtomicU32::new(0);

        with_entry!(2u64, e1, {
            with_entry!(2u64, e2, {
                unsafe { e1.as_mut() }.waker = Some(make_waker(&W1));
                unsafe { e2.as_mut() }.waker = Some(make_waker(&W2));
                unsafe { wheel.insert(e1) };
                unsafe { wheel.insert(e2) };

                wheel.tick(); // tick 0→1
                wheel.tick(); // tick 1→2  — fires deadline==2
                assert_eq!(W1.load(Ordering::SeqCst), 1);
                assert_eq!(W2.load(Ordering::SeqCst), 1);
            });
        });
    }

    #[test]
    fn cascade() {
        // A deadline of 8 (== SLOTS^1) lands on level 1 initially.
        // After 8 ticks the cascade should move it to level 0 and fire it.
        let mut wheel = Wheel::new();
        static WOKEN: AtomicU32 = AtomicU32::new(0);

        with_entry!(8u64, entry_ptr, {
            unsafe { entry_ptr.as_mut() }.waker = Some(make_waker(&WOKEN));
            unsafe { wheel.insert(entry_ptr) };

            for _ in 0..8 {
                wheel.tick();
            }
            assert_eq!(WOKEN.load(Ordering::SeqCst), 1, "cascade did not fire");
        });
    }
}
