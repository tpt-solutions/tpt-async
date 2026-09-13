//! [`Sleep`] — a future that resolves after a given number of ticks.

use core::future::Future;
use core::marker::PhantomPinned;
use core::pin::Pin;
use core::ptr::NonNull;
use core::task::{Context, Poll};

use crate::wheel::{TimerEntry, TimerWheel};

/// A future that resolves once the associated [`TimerWheel`] has advanced
/// past `deadline` ticks.
///
/// The wheel is only borrowed **shared** (`&TimerWheel`), so a `Sleep` can
/// coexist with a driver thread (or task) calling
/// [`tick`](TimerWheel::tick) — no exclusive borrow of the wheel is held.
///
/// `Sleep` is **not** [`Unpin`]: the intrusive entry inside it must not move
/// while it is registered in the wheel.  Pin it before polling (e.g. with
/// [`core::pin::pin!`] or inside an `async` block).
///
/// # Drop behaviour
/// Dropping `Sleep` while it is still pending automatically removes its
/// [`TimerEntry`] from the wheel, so no dangling pointer remains.
///
/// # Examples
///
/// ```rust,ignore
/// let wheel: TimerWheel<64, 4> = TimerWheel::new();
/// let sleep = pin!(Sleep::new(&wheel, wheel.now() + 10));
/// sleep.await; // resolves once the wheel has ticked 10 times
/// ```
pub struct Sleep<'a, const SLOTS: usize, const LEVELS: usize> {
    /// The (shared) wheel this future registers into.
    wheel: &'a TimerWheel<SLOTS, LEVELS>,
    /// Intrusive node stored inline — must not move after registration,
    /// which is what `PhantomPinned` enforces.
    entry: TimerEntry,
    /// Whether the entry is currently registered in the wheel.  Only touched
    /// by the polling thread; wheel-side deregistration is observable via
    /// `entry.is_registered()` under the wheel lock.
    registered: bool,
    /// Prevent the future from being `Unpin` while it may be registered.
    _pin: PhantomPinned,
}

impl<'a, const SLOTS: usize, const LEVELS: usize> Sleep<'a, SLOTS, LEVELS> {
    /// Create a new `Sleep` future that resolves when `wheel.now() >= deadline`.
    ///
    /// The returned future is not yet registered in the wheel; registration
    /// happens lazily on the first [`poll`](Future::poll).  A deadline at or
    /// before the current tick resolves on the first poll without ever
    /// touching the wheel.
    pub fn new(wheel: &'a TimerWheel<SLOTS, LEVELS>, deadline: u64) -> Self {
        Self {
            wheel,
            entry: TimerEntry::new(deadline),
            registered: false,
            _pin: PhantomPinned,
        }
    }

    /// The absolute tick this future resolves at.
    #[must_use]
    pub fn deadline(&self) -> u64 {
        self.entry.deadline()
    }

    /// Re-arm the sleep to a new absolute deadline, deregistering any stale
    /// registration.
    ///
    /// Used by [`Interval`](crate::interval::Interval); requires `&mut self`,
    /// which guarantees the future is not currently being polled.
    pub fn reset(&mut self, deadline: u64) {
        let ptr = NonNull::from(&mut self.entry);
        // Lock-protected removal: no-op if never registered or already
        // drained.
        self.wheel.inner.lock().remove(ptr);
        self.entry = TimerEntry::new(deadline);
        self.registered = false;
    }
}

impl<const SLOTS: usize, const LEVELS: usize> Future for Sleep<'_, SLOTS, LEVELS> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        // SAFETY: we never move any field out of `self` and never hand out
        // `&mut` to the pinned `entry`; we only mutate through `this`, whose
        // address is stable because `Sleep` is `!Unpin`.
        let this = unsafe { self.get_unchecked_mut() };

        let mut wheel = this.wheel.inner.lock();

        // Fast path: deadline already passed (including deadline <= now at
        // creation).  Deregister if we ever registered.  Everything here is
        // under the wheel lock, so the driver thread cannot race us.
        if wheel.now() >= this.entry.deadline() {
            let ptr = NonNull::from(&mut this.entry);
            wheel.remove(ptr); // no-op if never registered
            this.registered = false;
            return Poll::Ready(());
        }

        // Update the stored waker in case the executor changed it.  This must
        // happen under the wheel lock: the driver thread reads `entry.waker`
        // while draining, so all entry mutation is serialized here.
        let waker = cx.waker();
        if this
            .entry
            .waker
            .as_ref()
            .map_or(true, |w| !w.will_wake(waker))
        {
            this.entry.waker = Some(waker.clone());
        }

        if !this.registered {
            // Register: the entry is pinned at a stable address and we hold
            // the lock, so the wheel cannot have drained it between the check
            // above and here (we were not registered yet anyway).
            let ptr = NonNull::from(&mut this.entry);
            wheel.insert(ptr);
            this.registered = true;
            // Let the std driver know a new deadline exists so it can wake up
            // and recompute its sleep duration.  No-op on other targets.
            drop(wheel);
            #[cfg(feature = "std")]
            crate::driver::on_registration();
            return Poll::Pending;
        }

        Poll::Pending
    }
}

impl<const SLOTS: usize, const LEVELS: usize> Drop for Sleep<'_, SLOTS, LEVELS> {
    fn drop(&mut self) {
        let ptr = NonNull::from(&mut self.entry);
        // In drop the entry cannot be mid-move, and remove() is a no-op if
        // the wheel already drained the entry.
        self.wheel.inner.lock().remove(ptr);
        self.registered = false;
    }
}

impl<const SLOTS: usize, const LEVELS: usize> core::fmt::Debug for Sleep<'_, SLOTS, LEVELS> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Sleep")
            .field("deadline", &self.entry.deadline())
            .field("registered", &self.registered)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wheel::TimerWheel;
    use core::sync::atomic::{AtomicU32, Ordering};
    use core::task::{Context, RawWaker, RawWakerVTable, Waker};

    type Wheel = TimerWheel<8, 4>;

    fn counting_waker(counter: &'static AtomicU32) -> Waker {
        static VTABLE: RawWakerVTable = RawWakerVTable::new(
            |ptr| RawWaker::new(ptr, &VTABLE),
            |ptr| {
                // SAFETY: ptr is a valid `*const AtomicU32` cast.
                unsafe { &*(ptr as *const AtomicU32) }.fetch_add(1, Ordering::SeqCst);
            },
            |ptr| {
                // SAFETY: ptr is a valid `*const AtomicU32` cast.
                unsafe { &*(ptr as *const AtomicU32) }.fetch_add(1, Ordering::SeqCst);
            },
            |_| {},
        );
        // SAFETY: counter is 'static.
        unsafe {
            Waker::from_raw(RawWaker::new(
                counter as *const AtomicU32 as *const (),
                &VTABLE,
            ))
        }
    }

    fn cx_for(waker: &Waker) -> Context<'_> {
        Context::from_waker(waker)
    }

    #[test]
    fn past_deadline_resolves_immediately() {
        let wheel = Wheel::new();
        static COUNT: AtomicU32 = AtomicU32::new(0);
        let waker = counting_waker(&COUNT);
        let mut cx = cx_for(&waker);

        wheel.advance_to(7);
        let mut fut = core::pin::pin!(Sleep::new(&wheel, 3));
        assert!(fut.as_mut().poll(&mut cx).is_ready());
        assert!(wheel.is_empty());
    }

    #[test]
    fn sleep_fires_when_wheel_ticks() {
        let wheel = Wheel::new();
        static COUNT: AtomicU32 = AtomicU32::new(0);
        let waker = counting_waker(&COUNT);
        let mut cx = cx_for(&waker);

        let mut fut = core::pin::pin!(Sleep::new(&wheel, 3));
        assert!(fut.as_mut().poll(&mut cx).is_pending());
        assert_eq!(wheel.len(), 1, "not registered after first poll");

        wheel.tick();
        assert!(fut.as_mut().poll(&mut cx).is_pending());
        wheel.tick();
        assert!(fut.as_mut().poll(&mut cx).is_pending());
        wheel.tick(); // now == 3
        assert!(fut.as_mut().poll(&mut cx).is_ready());
        assert!(wheel.is_empty(), "drop-leftover registration");
    }

    #[test]
    fn drop_deregisters() {
        let wheel = Wheel::new();
        static COUNT: AtomicU32 = AtomicU32::new(0);
        let waker = counting_waker(&COUNT);
        let mut cx = cx_for(&waker);

        {
            let mut fut = core::pin::pin!(Sleep::new(&wheel, 5));
            assert!(fut.as_mut().poll(&mut cx).is_pending());
            assert_eq!(wheel.len(), 1);
            // The `Sleep` drops at the end of this block (`drop` on the
            // `Pin` handle alone would not run its `Drop`).
        }
        assert!(wheel.is_empty(), "drop must deregister");

        // Ticking afterwards must not panic (no dangling entry).
        for _ in 0..6 {
            wheel.tick();
        }
    }
}
