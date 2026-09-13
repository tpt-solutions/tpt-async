//! Waker construction utilities.
//!
//! These are useful for testing, embedded environments, and building custom
//! executors without depending on heap allocation.

use core::task::{RawWaker, RawWakerVTable, Waker};

// ── no-op waker ──────────────────────────────────────────────────────────────

static NOOP_VTABLE: RawWakerVTable = RawWakerVTable::new(
    |_| RawWaker::new(core::ptr::null(), &NOOP_VTABLE), // clone
    |_| {},                                             // wake
    |_| {},                                             // wake_by_ref
    |_| {},                                             // drop
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
    // The Waker lives in a process-wide `spin::Once` and is never freed;
    // the noop vtable makes cloning it free, so sharing one instance is safe.
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
        // SAFETY: same as above.
        |ptr| unsafe { core::mem::transmute::<*const (), fn()>(ptr)() },
        |_| {},
    );

    // SAFETY: `data` is a function pointer; VTABLE only ever calls it or clones
    // the pointer — no memory at the address is accessed.
    unsafe { Waker::from_raw(RawWaker::new(data, &VTABLE)) }
}

/// Creates a [`Waker`] that invokes an arbitrary closure when woken.
///
/// Unlike [`waker_fn`] (which only accepts stateless `fn()` pointers), this
/// accepts any closure with captured state, at the cost of one `Arc`
/// allocation for the waker body.  Requires the **`alloc`** feature.
///
/// # Example
///
/// ```rust
/// use tpt_async_core::waker::waker_with;
/// use core::sync::atomic::{AtomicBool, Ordering};
/// use std::sync::Arc;
///
/// let flag = Arc::new(AtomicBool::new(false));
/// let flag2 = Arc::clone(&flag);
/// let waker = waker_with(move || flag2.store(true, Ordering::SeqCst));
/// waker.wake_by_ref();
/// assert!(flag.load(Ordering::SeqCst));
/// ```
#[cfg(feature = "alloc")]
pub fn waker_with<W>(on_wake: W) -> Waker
where
    W: Fn() + Send + Sync + 'static,
{
    use alloc::sync::Arc;
    use core::task::{RawWaker, RawWakerVTable};

    /// The heap-allocated waker body.  The vtable is stored *by value* inside
    /// the body so that the (per-instantiation, non-`static`) table survives
    /// as long as the waker itself: clones read it from the data pointer.
    struct Body<W> {
        on_wake: W,
        vtable: RawWakerVTable,
    }

    // SAFETY: `data` is an `Arc::into_raw` pointer with one owned reference
    // belonging to the waker machinery.  Reconstructing the Arc transfers
    // that reference back to us; the closure runs, then the Arc drops.
    unsafe fn wake_owned<W>(data: *const ())
    where
        W: Fn() + Send + Sync + 'static,
    {
        let arc = unsafe { Arc::from_raw(data as *const Body<W>) };
        (arc.on_wake)();
        // `arc` drops here, releasing the reference.
    }

    unsafe fn wake_by_ref<W>(data: *const ())
    where
        W: Fn() + Send + Sync + 'static,
    {
        // SAFETY: only a temporary clone is taken; the waker's own reference
        // count is left unchanged.
        let arc = unsafe { Arc::from_raw(data as *const Body<W>) };
        (arc.on_wake)();
        core::mem::forget(arc);
    }

    unsafe fn clone_waker<W>(data: *const ()) -> RawWaker
    where
        W: Fn() + Send + Sync + 'static,
    {
        // SAFETY: temporary clone for the refcount bump, forgotten again.
        let arc = unsafe { Arc::from_raw(data as *const Body<W>) };
        let cloned = Arc::clone(&arc);
        core::mem::forget(arc);
        // Every body carries its own copy of the vtable, so the new waker is
        // self-contained: the reference points into `cloned`, which the new
        // RawWaker keeps alive.
        let cloned_ptr = Arc::into_raw(cloned) as *const ();
        let vtable = unsafe { &(*cloned_ptr.cast::<Body<W>>()).vtable };
        RawWaker::new(cloned_ptr, vtable)
    }

    unsafe fn drop_waker<W>(data: *const ())
    where
        W: Fn() + Send + Sync + 'static,
    {
        // SAFETY: transfers the waker's owned reference to `drop`.
        drop(unsafe { Arc::from_raw(data as *const Body<W>) });
    }

    // The table entries are non-capturing closures that monomorphize to the
    // generic helpers for this `W`; a per-call table is equivalent to a
    // static one, and a copy of it is stored in every waker body.
    let vtable = RawWakerVTable::new(
        // SAFETY: the helpers uphold the RawWaker contract (see each).
        |ptr| unsafe { clone_waker::<W>(ptr) },
        |ptr| unsafe { wake_owned::<W>(ptr) },
        |ptr| unsafe { wake_by_ref::<W>(ptr) },
        |ptr| unsafe { drop_waker::<W>(ptr) },
    );

    let body = Arc::new(Body { on_wake, vtable });
    let data = Arc::into_raw(body) as *const ();
    // SAFETY: `data` owns one reference, and the vtable reference points into
    // that same body, which the waker keeps alive for its whole lifetime.
    unsafe { Waker::from_raw(RawWaker::new(data, &(*data.cast::<Body<W>>()).vtable)) }
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
        let w = waker_fn(|| {
            COUNT.fetch_add(1, Ordering::SeqCst);
        });
        w.wake_by_ref();
        w.wake_by_ref();
        assert_eq!(COUNT.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn waker_fn_clone_works() {
        static COUNT: AtomicU32 = AtomicU32::new(0);
        let w = waker_fn(|| {
            COUNT.fetch_add(1, Ordering::SeqCst);
        });
        let w2 = w.clone();
        w2.wake();
        assert_eq!(COUNT.load(Ordering::SeqCst), 1);
    }
}
