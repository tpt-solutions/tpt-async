//! [`Sleep`] — a future that resolves after a given number of ticks.

use core::future::Future;
use core::marker::PhantomPinned;
use core::pin::Pin;
use core::ptr::NonNull;
use core::task::{Context, Poll};

use crate::wheel::{TimerEntry, TimerWheel};

/// A future that resolves once the associated [`TimerWheel`] has advanced past
/// `deadline` ticks.
///
/// `Sleep` is **not** [`Unpin`]; it must be pinned before it can be polled.
///
/// # Drop behaviour
/// Dropping `Sleep` while it is still pending automatically removes its
/// [`TimerEntry`] from the wheel so no dangling pointer remains.
pub struct Sleep<'wheel, const SLOTS: usize, const LEVELS: usize> {
    /// Non-owning pointer to the wheel; the wheel must outlive this future.
    wheel: *mut TimerWheel<SLOTS, LEVELS>,
    /// Intrusive node stored inline — must never move after first poll.
    entry: TimerEntry,
    /// Whether the entry is currently in the wheel's linked lists.
    registered: bool,
    /// Prevent the future from being `Unpin`.
    _pin: PhantomPinned,
    /// Tie the lifetime to the wheel reference.
    _wheel_lt: core::marker::PhantomData<&'wheel mut TimerWheel<SLOTS, LEVELS>>,
}

impl<'wheel, const SLOTS: usize, const LEVELS: usize>
    Sleep<'wheel, SLOTS, LEVELS>
{
    /// Create a new `Sleep` future that resolves when `wheel.now() >= deadline`.
    ///
    /// The returned future is not yet registered in the wheel; registration
    /// happens lazily on the first [`poll`](Future::poll).
    pub fn new(
        wheel: &'wheel mut TimerWheel<SLOTS, LEVELS>,
        deadline: u64,
    ) -> Self {
        Self {
            wheel: wheel as *mut _,
            entry: TimerEntry::new(deadline),
            registered: false,
            _pin: PhantomPinned,
            _wheel_lt: core::marker::PhantomData,
        }
    }
}

impl<'wheel, const SLOTS: usize, const LEVELS: usize> Future
    for Sleep<'wheel, SLOTS, LEVELS>
{
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        // SAFETY: we never move any field out of `self`; we only read and write
        // through the pinned pointer.
        let this = unsafe { self.get_unchecked_mut() };

        // SAFETY: `this.wheel` was obtained from a valid `&mut TimerWheel`
        // whose lifetime ('wheel) is tied to this future.
        let wheel = unsafe { &mut *this.wheel };

        // If the deadline has already passed, resolve immediately.
        if wheel.now() >= this.entry.deadline {
            if this.registered {
                let entry_ptr =
                    // SAFETY: `entry` is pinned (we are inside a Pin<&mut Self>)
                    // and is currently registered.
                    unsafe { NonNull::new_unchecked(&mut this.entry as *mut _) };
                // SAFETY: same pointer, still pinned.
                unsafe { wheel.remove(entry_ptr) };
                this.registered = false;
            }
            return Poll::Ready(());
        }

        // Update the stored waker in case the executor changed it.
        if let Some(ref w) = this.entry.waker {
            if !w.will_wake(cx.waker()) {
                this.entry.waker = Some(cx.waker().clone());
                // If already registered, update waker in place (no re-insert needed
                // because the slot is determined by the deadline, which is fixed).
            }
        } else {
            this.entry.waker = Some(cx.waker().clone());
        }

        if !this.registered {
            let entry_ptr =
                // SAFETY: `entry` is part of a pinned `Self` and is not currently
                // registered.
                unsafe { NonNull::new_unchecked(&mut this.entry as *mut _) };
            // SAFETY: entry is pinned and not registered.
            unsafe { wheel.insert(entry_ptr) };
            this.registered = true;
        }

        Poll::Pending
    }
}

impl<'wheel, const SLOTS: usize, const LEVELS: usize> Drop
    for Sleep<'wheel, SLOTS, LEVELS>
{
    fn drop(&mut self) {
        if self.registered {
            let entry_ptr =
                // SAFETY: `entry` is still at its original address (we haven't
                // moved it) and is registered in the wheel.
                unsafe { NonNull::new_unchecked(&mut self.entry as *mut _) };
            // SAFETY: `self.wheel` is still valid (the wheel outlives `Sleep`
            // per the `'wheel` lifetime bound).
            let wheel = unsafe { &mut *self.wheel };
            // SAFETY: entry is still pinned (we are in `drop`, not `move`).
            unsafe { wheel.remove(entry_ptr) };
            self.registered = false;
        }
    }
}
