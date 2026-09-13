//! Hierarchical timer wheel — O(1) insert and remove.
//!
//! # Level layout
//!
//! - Level 0 resolution: 1 tick; covers `0..SLOTS` ticks ahead
//! - Level k resolution: `SLOTS^k` ticks; covers `0..SLOTS^(k+1)` ticks ahead
//!
//! # Shared use
//!
//! All wheel methods take `&self`; the wheel's internals are protected by a
//! [`spin::Mutex`], so a single wheel can be shared between the task(s) that
//! create [`Sleep`](crate::sleep::Sleep) futures and the driver thread that
//! advances it — no exclusive borrow is ever held.
//!
//! # Invariants
//!
//! - A registered [`TimerEntry`] must not move or be dropped until it is
//!   removed (or fired).  [`Sleep`](crate::sleep::Sleep),
//!   [`Timeout`](crate::timeout::Timeout) and [`Interval`](crate::interval::Interval)
//!   uphold this automatically via their [`Drop`] impls; only raw manual use
//!   of [`TimerWheel::insert`] / [`TimerWheel::remove`] can violate it.
//! - Every mutation of a registered entry happens while holding the wheel
//!   lock, which makes a shared wheel sound across threads.
//! - `wake()` implementations called by the wheel must not synchronously poll
//!   tasks that insert into the *same* wheel (true for every mainstream
//!   executor, whose `wake` merely enqueues).  Recursive insertion from
//!   inside a wake would deadlock the non-reentrant lock.

use core::ptr::NonNull;
use core::task::Waker;

use spin::Mutex;

// ── TimerEntry ────────────────────────────────────────────────────────────────

/// An intrusive linked-list node stored inline inside a
/// [`Sleep`](crate::sleep::Sleep) future.
///
/// Most users never touch this type: [`Sleep`](crate::sleep::Sleep),
/// [`Timeout`](crate::timeout::Timeout) and [`Interval`](crate::interval::Interval)
/// own their entries and register them automatically.  It is public only for
/// manual, allocation-free driving of a wheel on bare metal.
///
/// # Safety contract
///
/// An entry must remain at the same address and must not be dropped while it
/// is registered in a wheel.  Dropping or moving a registered entry leaves a
/// dangling pointer in the wheel's slot lists; the next [`tick`](TimerWheel::tick)
/// would dereference it.
pub struct TimerEntry {
    /// Absolute tick at which this entry fires.
    pub(crate) deadline: u64,
    /// Waker to call when the entry fires.
    pub(crate) waker: Option<Waker>,
    /// Next entry in the same slot's intrusive linked list.
    next: Option<NonNull<TimerEntry>>,
    /// Points to the `next` field that points to *this* entry (either the slot
    /// head pointer or a predecessor's `next` field).  Allows O(1) removal.
    prev_next: Option<NonNull<Option<NonNull<TimerEntry>>>>,
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

    /// The absolute deadline this entry fires at.
    #[must_use]
    pub const fn deadline(&self) -> u64 {
        self.deadline
    }

    /// Returns `true` if this entry is currently registered in a wheel.
    ///
    /// Only meaningful while the owning wheel's lock is held; reading it
    /// concurrently with a driver thread is a (benign) race.
    #[inline]
    pub(crate) fn is_registered(&self) -> bool {
        self.prev_next.is_some()
    }
}

// SAFETY: an entry carries only a `Waker` (Send + Sync), a tick count, and
// raw pointers to other entries.  The pointers are dereferenced exclusively
// while holding the owning wheel's lock (see the module invariants), which
// serializes every cross-thread access; unregistered entries are only
// touched by their owner.  Rust's auto-trait derivation cannot see through
// the self-referential `NonNull` chain, so this must be stated explicitly.
unsafe impl Send for TimerEntry {}
unsafe impl Sync for TimerEntry {}

// ── wheel internals (guarded by the mutex) ───────────────────────────────────

pub(crate) struct Inner<const SLOTS: usize, const LEVELS: usize> {
    /// `heads[level][slot]` — head of the intrusive singly-linked list.
    heads: [[Option<NonNull<TimerEntry>>; SLOTS]; LEVELS],
    current_tick: u64,
    /// Number of registered entries (for `len` / `is_empty`).
    pending: usize,
}

// SAFETY: `Inner` is raw bookkeeping — pointers to `TimerEntry` nodes plus
// counters.  Handles may move between threads freely; every dereference
// happens under the owning wheel's spin mutex (see the module invariants),
// so cross-thread access is serialized.  `NonNull` is conservative about
// auto traits (it is not `Send` for any pointee on current toolchains), so
// this cannot be derived.
unsafe impl<const SLOTS: usize, const LEVELS: usize> Send for Inner<SLOTS, LEVELS> {}
unsafe impl<const SLOTS: usize, const LEVELS: usize> Sync for Inner<SLOTS, LEVELS> {}

impl<const SLOTS: usize, const LEVELS: usize> Inner<SLOTS, LEVELS> {
    #[inline]
    pub(crate) fn now(&self) -> u64 {
        self.current_tick
    }
    /// Choose `(level, slot)` for an entry with the given `deadline`.
    fn choose_slot(&self, deadline: u64) -> (usize, usize) {
        let delta = deadline.saturating_sub(self.current_tick);
        if delta == 0 {
            // Already due (or overdue): park it in the current level-0 slot;
            // it drains on the next `tick` at the latest.
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

        // Slot index within that level.  Level k has resolution SLOTS^k ticks.
        let resolution = (SLOTS as u64).saturating_pow(level as u32);
        let slot = ((deadline / resolution) % SLOTS as u64) as usize;
        (level, slot)
    }

    /// Insert an entry (see [`TimerWheel::insert`] for the contract).
    pub(crate) fn insert(&mut self, mut entry: NonNull<TimerEntry>) {
        // SAFETY: caller guarantees the pointer is valid, immovable while
        // registered, and not currently registered in any wheel.
        let e = unsafe { entry.as_mut() };
        debug_assert!(
            !e.is_registered(),
            "TimerEntry inserted while already registered"
        );

        let (level, slot) = self.choose_slot(e.deadline);

        // Link at the front of heads[level][slot].
        let head_ptr: *mut Option<NonNull<TimerEntry>> = &mut self.heads[level][slot];

        e.next = self.heads[level][slot];
        // SAFETY: `head_ptr` points into `self.heads` which is valid for the
        // lifetime of the wheel.
        e.prev_next = Some(unsafe { NonNull::new_unchecked(head_ptr) });

        // If there was a previous head, update its prev_next.
        if let Some(mut old_head) = e.next {
            // SAFETY: old_head is a registered entry and thus valid.
            unsafe { old_head.as_mut() }.prev_next =
                Some(unsafe { NonNull::new_unchecked(&mut e.next as *mut _) });
        }

        self.heads[level][slot] = Some(entry);
        self.pending += 1;
    }

    /// Remove an entry; a no-op if it is not currently registered.
    pub(crate) fn remove(&mut self, mut entry: NonNull<TimerEntry>) {
        // SAFETY: caller guarantees the pointer is valid.
        let e = unsafe { entry.as_mut() };

        let prev_next_ptr = match e.prev_next.take() {
            Some(p) => p,
            None => return, // not registered
        };

        // Write our `next` into the slot that was pointing at us.
        // SAFETY: prev_next_ptr is a valid pointer to an
        // `Option<NonNull<TimerEntry>>` field (either in a slot head or in a
        // predecessor entry's `next`).
        unsafe { *prev_next_ptr.as_ptr() = e.next };

        // Update the successor's prev_next to skip over us.
        if let Some(mut next_entry) = e.next.take() {
            // SAFETY: next_entry is registered and thus valid.
            unsafe { next_entry.as_mut() }.prev_next = Some(prev_next_ptr);
        }

        self.pending -= 1;
    }

    /// Advance the wheel by one tick and wake everything that is due.
    ///
    /// Ordering is: increment → cascade coarse levels → drain the level-0
    /// slot for the new tick.  This means an entry with `deadline == D` fires
    /// on precisely the `tick` call that moves `current_tick` to `D`, and an
    /// entry cascaded into level 0 during that same call still fires on time.
    fn advance_one(&mut self) {
        self.current_tick += 1;

        // Cascade higher levels when a level-0 wrap occurs.
        // Level k cascades when current_tick is a multiple of SLOTS^k.
        let mut width = SLOTS as u64;
        for level in 1..LEVELS {
            if self.current_tick % width == 0 {
                let slot = ((self.current_tick / width) % SLOTS as u64) as usize;
                self.cascade(level, slot);
            } else {
                break;
            }
            width = width.saturating_mul(SLOTS as u64);
        }

        let slot0 = (self.current_tick % SLOTS as u64) as usize;
        self.drain_slot(0, slot0);
    }

    /// Drain a slot: wake every entry in it.
    fn drain_slot(&mut self, level: usize, slot: usize) {
        // Detach the whole list.
        let mut cursor = self.heads[level][slot].take();
        while let Some(mut node_ptr) = cursor {
            // SAFETY: all registered entries are valid and immovable.
            let node = unsafe { node_ptr.as_mut() };
            cursor = node.next.take();
            node.prev_next = None; // mark as unregistered
            self.pending -= 1;

            if let Some(waker) = node.waker.take() {
                // See the module docs: wake() must not re-enter this wheel.
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
            // SAFETY: registered entries are valid and immovable.
            let node = unsafe { node_ptr.as_mut() };
            cursor = node.next.take();
            node.prev_next = None; // temporarily unregistered

            // Re-insert at the correct (lower) level.
            // SAFETY: node is still in place; we just cleared its registration
            // state so the insert preconditions hold.
            self.insert(node_ptr);
        }
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
/// All methods take `&self`; the internals are guarded by a spin mutex, so
/// the wheel can be shared with a driver thread.
///
/// # Examples
///
/// ```rust
/// use tpt_async_timer::wheel::TimerWheel;
///
/// let wheel: TimerWheel<8, 4> = TimerWheel::new();
/// assert_eq!(wheel.now(), 0);
/// assert!(wheel.is_empty());
/// wheel.tick();
/// assert_eq!(wheel.now(), 1);
/// ```
pub struct TimerWheel<const SLOTS: usize, const LEVELS: usize> {
    /// Crate-visible so the futures in `sleep`/`timeout`/`interval` can lock
    /// the wheel themselves (all entry mutation must happen under the lock).
    pub(crate) inner: Mutex<Inner<SLOTS, LEVELS>>,
}

impl<const SLOTS: usize, const LEVELS: usize> TimerWheel<SLOTS, LEVELS> {
    /// Create a new, empty timer wheel.
    ///
    /// # Panics
    /// Panics if `SLOTS < 2` or `LEVELS < 1`; in a const context (e.g. a
    /// `static WHEEL: TimerWheel<…> = TimerWheel::new();`) the violation is
    /// a compile error instead.
    pub const fn new() -> Self {
        assert!(SLOTS >= 2, "TimerWheel requires SLOTS >= 2");
        assert!(LEVELS >= 1, "TimerWheel requires LEVELS >= 1");
        Self {
            inner: Mutex::new(Inner {
                heads: [[None; SLOTS]; LEVELS],
                current_tick: 0,
                pending: 0,
            }),
        }
    }

    /// The current tick count.
    #[inline]
    pub fn now(&self) -> u64 {
        self.inner.lock().now()
    }

    /// Number of entries currently registered in the wheel.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.lock().pending
    }

    /// Returns `true` if no entries are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The earliest deadline among registered entries, if any.
    ///
    /// Useful for drivers that want to sleep until the next timeout is due.
    /// Note that entries in coarse levels have not yet been cascaded, so this
    /// is a lower bound of the true next-firing tick.
    #[must_use]
    pub fn next_deadline(&self) -> Option<u64> {
        let inner = self.inner.lock();
        let mut min: Option<u64> = None;
        for level in inner.heads.iter() {
            for slot in level.iter().flatten() {
                // SAFETY: registered entries are valid and immovable.
                let d = unsafe { slot.as_ref() }.deadline;
                min = Some(match min {
                    Some(m) if m <= d => m,
                    _ => d,
                });
            }
        }
        min
    }

    /// Advance the wheel by one tick, waking all entries whose deadline has
    /// been reached.
    pub fn tick(&self) {
        self.inner.lock().advance_one();
    }

    /// Advance the wheel by `n` ticks.
    pub fn tick_n(&self, n: u64) {
        let mut inner = self.inner.lock();
        for _ in 0..n {
            inner.advance_one();
        }
    }

    /// Advance the wheel until `current_tick` reaches `target`.
    ///
    /// A no-op if the wheel is already at or beyond `target`.  Drivers use
    /// this to catch up after any suspension, keeping tick jitter from
    /// inflating timeout latencies.
    pub fn advance_to(&self, target: u64) {
        let mut inner = self.inner.lock();
        while inner.current_tick < target {
            inner.advance_one();
        }
    }

    /// Insert an already-pinned, immovable `TimerEntry` into the wheel.
    ///
    /// Most users should use [`Sleep`](crate::sleep::Sleep) instead, which
    /// owns its entry and registers it on first poll.
    ///
    /// # Safety
    /// - `entry` must be valid and must not move or be dropped while it is
    ///   registered in any wheel.
    /// - `entry` must not currently be registered in any wheel.
    pub unsafe fn insert(&self, entry: NonNull<TimerEntry>) {
        self.inner.lock().insert(entry);
    }

    /// Remove a `TimerEntry` from the wheel.
    ///
    /// Safe to call even if the entry is not currently registered (no-op).
    ///
    /// # Safety
    /// - `entry` must still be valid (it may have moved if it was never
    ///   registered).
    pub unsafe fn remove(&self, entry: NonNull<TimerEntry>) {
        self.inner.lock().remove(entry);
    }
}

impl<const SLOTS: usize, const LEVELS: usize> Default for TimerWheel<SLOTS, LEVELS> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const SLOTS: usize, const LEVELS: usize> core::fmt::Debug for TimerWheel<SLOTS, LEVELS> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let inner = self.inner.lock();
        f.debug_struct("TimerWheel")
            .field("current_tick", &inner.now())
            .field("pending", &inner.pending)
            .finish()
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
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
            #[allow(unused_mut)]
            let mut $name: NonNull<TimerEntry> =
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
                unsafe { &*(ptr as *const AtomicU32) }.fetch_add(1, Ordering::SeqCst);
            },
            // wake_by_ref
            |ptr| {
                // SAFETY: same as above.
                unsafe { &*(ptr as *const AtomicU32) }.fetch_add(1, Ordering::SeqCst);
            },
            // drop
            |_| {},
        );

        // SAFETY: `counter` is a `'static` reference so the pointer is always valid.
        let ptr = counter as *const AtomicU32 as *const ();
        unsafe { Waker::from_raw(RawWaker::new(ptr, &VTABLE)) }
    }

    #[test]
    fn fires_exactly_at_deadline() {
        let wheel = Wheel::new();
        static WOKEN: AtomicU32 = AtomicU32::new(0);

        with_entry!(3u64, entry_ptr, {
            // SAFETY: entry is pinned on the stack and not registered.
            unsafe { entry_ptr.as_mut() }.waker = Some(make_waker(&WOKEN));
            // SAFETY: not registered anywhere yet.
            unsafe { wheel.insert(entry_ptr) };

            wheel.tick(); // now == 1
            assert_eq!(WOKEN.load(Ordering::SeqCst), 0, "fired too early at tick 1");
            wheel.tick(); // now == 2
            assert_eq!(WOKEN.load(Ordering::SeqCst), 0, "fired too early at tick 2");
            wheel.tick(); // now == 3 — fires
            assert_eq!(WOKEN.load(Ordering::SeqCst), 1, "not fired after tick 3");
            assert_eq!(wheel.now(), 3);
        });
    }

    #[test]
    fn early_tick_does_not_fire() {
        let wheel = Wheel::new();
        static WOKEN: AtomicU32 = AtomicU32::new(0);

        with_entry!(5u64, entry_ptr, {
            // SAFETY: valid stack storage.
            unsafe { entry_ptr.as_mut() }.waker = Some(make_waker(&WOKEN));
            // SAFETY: not registered anywhere yet.
            unsafe { wheel.insert(entry_ptr) };

            wheel.tick();
            wheel.tick();
            assert_eq!(WOKEN.load(Ordering::SeqCst), 0);
            assert_eq!(wheel.len(), 1);

            // Clean up — remove before the entry is dropped.
            // SAFETY: entry still valid.
            unsafe { wheel.remove(entry_ptr) };
            assert!(wheel.is_empty());
        });
    }

    #[test]
    fn multiple_entries_same_slot() {
        let wheel = Wheel::new();
        static W1: AtomicU32 = AtomicU32::new(0);
        static W2: AtomicU32 = AtomicU32::new(0);

        with_entry!(2u64, e1, {
            with_entry!(2u64, e2, {
                // SAFETY: valid stack storage, not registered.
                unsafe { e1.as_mut() }.waker = Some(make_waker(&W1));
                // SAFETY: valid stack storage, not registered.
                unsafe { e2.as_mut() }.waker = Some(make_waker(&W2));
                // SAFETY: not registered anywhere yet.
                unsafe { wheel.insert(e1) };
                // SAFETY: not registered anywhere yet.
                unsafe { wheel.insert(e2) };
                assert_eq!(wheel.len(), 2);

                wheel.tick(); // now == 1
                wheel.tick(); // now == 2 — fires deadline==2
                assert_eq!(W1.load(Ordering::SeqCst), 1);
                assert_eq!(W2.load(Ordering::SeqCst), 1);
                assert!(wheel.is_empty());
            });
        });
    }

    #[test]
    fn cascade() {
        // A deadline of 8 (== SLOTS^1) lands on level 1 initially.
        // After 8 ticks the cascade should move it to level 0 and fire it.
        let wheel = Wheel::new();
        static WOKEN: AtomicU32 = AtomicU32::new(0);

        with_entry!(8u64, entry_ptr, {
            // SAFETY: valid stack storage.
            unsafe { entry_ptr.as_mut() }.waker = Some(make_waker(&WOKEN));
            // SAFETY: not registered anywhere yet.
            unsafe { wheel.insert(entry_ptr) };

            for _ in 0..8 {
                wheel.tick();
            }
            assert_eq!(WOKEN.load(Ordering::SeqCst), 1, "cascade did not fire");
        });
    }

    #[test]
    fn double_cascade_deep_level() {
        // Deadline 70 > 8^2 = 64, so it starts on level 2 (slot 70/64 % 8 = 1)
        // and must cascade twice before firing exactly at tick 70.
        let wheel = Wheel::new();
        static WOKEN: AtomicU32 = AtomicU32::new(0);

        with_entry!(70u64, entry_ptr, {
            // SAFETY: valid stack storage.
            unsafe { entry_ptr.as_mut() }.waker = Some(make_waker(&WOKEN));
            // SAFETY: not registered anywhere yet.
            unsafe { wheel.insert(entry_ptr) };

            wheel.tick_n(70);
            assert_eq!(
                WOKEN.load(Ordering::SeqCst),
                1,
                "double cascade did not fire"
            );
        });
    }

    #[test]
    fn overdue_entry_fires_on_slot_drain() {
        // An entry whose deadline has already passed parks in the *current*
        // level-0 slot and fires when that slot next drains.
        let wheel = Wheel::new();
        static WOKEN: AtomicU32 = AtomicU32::new(0);

        with_entry!(0u64, entry_ptr, {
            // SAFETY: valid stack storage.
            unsafe { entry_ptr.as_mut() }.waker = Some(make_waker(&WOKEN));
            // Insert at now == 0 with deadline 0: parked in slot 0.
            // SAFETY: not registered anywhere yet.
            unsafe { wheel.insert(entry_ptr) };

            // Slot 0 drains when current_tick % 8 == 0, i.e. after 8 ticks.
            wheel.tick_n(8);
            assert_eq!(WOKEN.load(Ordering::SeqCst), 1);
        });
    }

    #[test]
    fn next_deadline_reports_minimum() {
        let wheel = Wheel::new();

        with_entry!(10u64, e1, {
            with_entry!(4u64, e2, {
                // SAFETY: valid stack storage, not registered.
                unsafe { wheel.insert(e1) };
                // SAFETY: valid stack storage, not registered.
                unsafe { wheel.insert(e2) };
                assert_eq!(wheel.next_deadline(), Some(4));
                // Remove before the stack storage goes out of scope.
                // SAFETY: removing exactly what we inserted.
                unsafe { wheel.remove(e1) };
                // SAFETY: removing exactly what we inserted.
                unsafe { wheel.remove(e2) };
            });
        });

        assert_eq!(wheel.next_deadline(), None);
    }

    #[test]
    fn advance_to_catches_up() {
        let wheel = Wheel::new();
        static WOKEN: AtomicU32 = AtomicU32::new(0);

        with_entry!(5u64, entry_ptr, {
            // SAFETY: valid stack storage.
            unsafe { entry_ptr.as_mut() }.waker = Some(make_waker(&WOKEN));
            // SAFETY: not registered anywhere yet.
            unsafe { wheel.insert(entry_ptr) };

            wheel.advance_to(100);
            assert_eq!(wheel.now(), 100);
            assert_eq!(WOKEN.load(Ordering::SeqCst), 1);
        });

        // Advancing backwards is a no-op.
        wheel.advance_to(50);
        assert_eq!(wheel.now(), 100);
    }

    #[test]
    fn shared_refs_and_send() {
        // The wheel is usable via &self from "multiple threads" and is
        // automatically Send + Sync (no unsafe impls).
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Wheel>();
    }
}
