//! Waker construction utilities.
//!
//! These are useful for testing, embedded environments, and building custom
//! executors without depending on heap allocation.

use core::task::{RawWaker, RawWakerVTable, Waker};

// ── no-op waker ──────────────────────────────────────────────────────────────

static NOOP_VTABLE: RawWakerVTable = RawWakerVTable::new(
    |_| RawWaker::new(core::ptr::null(), &NOOP_VTABLE), // clone
    |_| {},                                              // wake
    |_| {},                                              // wake_by_ref
    |_| {},                                              // drop
);

/// Returns a [`Waker`] that does nothing when woken.
///
/// Useful in tests or as a sentinel when no real notification mechanism is
/// needed.
pub fn noop_waker() -> Waker {
    // SAFETY: NOOP_VTABLE never dereferences the data pointer.
    unsafe { Waker::from_raw(RawWaker::new(core::ptr::null(), &NOOP_VTABLE)) }
}

/// Returns a [`core::task::Context`] backed by a no-op waker.
pub fn noop_context() -> core::task::Context<'static> {
    // `noop_waker()` leaks a static; we leak the Waker too to get a `'static` ref.
    // This is intentionally limited to tests / no-runtime paths.
    static NOOP: spin::Once<Waker> = spin::Once::new();
    let waker = NOOP.call_once(noop_waker);
    core::task::Context::from_waker(waker)
}

// ── function-pointer waker ────────────────────────────────────────────────────

/// Creates a [`Waker`] that calls `wake_fn` whenever `wake()` or
/// `wake_by_ref()` is invoked.
///
/// `wake_fn` must be a stateless function pointer (`fn()`), so this waker
/// requires no heap allocation.
///
/// # Example
///
/// ```rust
/// use tpt_async_core::waker::waker_fn;
/// use core::sync::atomic::{AtomicBool, Ordering};
///
/// static WOKEN: AtomicBool = AtomicBool::new(false);
/// let waker = waker_fn(|| WOKEN.store(true, Ordering::SeqCst));
/// waker.wake_by_ref();
/// assert!(WOKEN.load(Ordering::SeqCst));
/// ```
pub fn waker_fn(wake_fn: fn()) -> Waker {
    // Store the function pointer as the data pointer (it's just a usize).
    let data = wake_fn as *const ();

    static VTABLE: RawWakerVTable = RawWakerVTable::new(
        |ptr| RawWaker::new(ptr, &VTABLE),
        // SAFETY: ptr is a valid `fn()` function pointer cast to `*const ()`.
        |ptr| unsafe { core::mem::transmute::<*const (), fn()>(ptr)() },
        |ptr| unsafe { core::mem::transmute::<*const (), fn()>(ptr)() },
        |_| {},
    );

    // SAFETY: `data` is a function pointer; VTABLE only ever calls it or clones
    // the pointer — no memory at the address is accessed.
    unsafe { Waker::from_raw(RawWaker::new(data, &VTABLE)) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::{AtomicU32, Ordering};

    #[test]
    fn noop_waker_does_nothing() {
        let w = noop_waker();
        w.wake_by_ref(); // must not panic
    }

    #[test]
    fn waker_fn_calls_on_wake() {
        static COUNT: AtomicU32 = AtomicU32::new(0);
        let w = waker_fn(|| { COUNT.fetch_add(1, Ordering::SeqCst); });
        w.wake_by_ref();
        w.wake_by_ref();
        assert_eq!(COUNT.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn waker_fn_clone_works() {
        static COUNT: AtomicU32 = AtomicU32::new(0);
        let w = waker_fn(|| { COUNT.fetch_add(1, Ordering::SeqCst); });
        let w2 = w.clone();
        w2.wake();
        assert_eq!(COUNT.load(Ordering::SeqCst), 1);
    }
}
