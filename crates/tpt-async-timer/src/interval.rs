//! [`Interval`] — a stream-like future that fires repeatedly at a fixed
//! period.

use core::future::Future;
use core::marker::PhantomPinned;
use core::pin::Pin;
use core::task::{Context, Poll};

use crate::sleep::Sleep;
use crate::wheel::TimerWheel;

/// How an [`Interval`] catches up after a tick was missed (the task was busy
/// for longer than `period`, or the wheel advanced while nothing polled).
///
/// The default is [`MissedTickBehavior::Skip`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MissedTickBehavior {
    /// Fire as fast as needed to catch up to the original schedule:
    /// the next deadline is `old_deadline + period` even if that is already
    /// in the past.  Bursts of rapid ticks follow a long suspension.
    Burst,
    /// Shift the whole schedule: the next deadline is `now + period`.
    /// A long suspension cancels all missed ticks and restarts the rhythm.
    Delay,
    /// Skip missed ticks but stay on the original grid: the next deadline is
    /// the first multiple of `period` strictly after `now`.  Ticks keep
    /// aligned to wall-clock boundaries (relative to wheel start).
    #[default]
    Skip,
}

impl MissedTickBehavior {
    /// Compute the next deadline after `old` was missed at wheel time `now`.
    #[must_use]
    pub fn next_deadline(self, old: u64, period: u64, now: u64) -> u64 {
        match self {
            Self::Burst => old.saturating_add(period),
            Self::Delay => now.saturating_add(period),
            Self::Skip => {
                let aligned = (now / period) * period + period;
                aligned.max(old.saturating_add(period))
            }
        }
    }
}

/// Yields `()` repeatedly, every `period` ticks of the associated wheel.
///
/// Like [`Sleep`], the wheel is borrowed shared, so a
/// driver thread may keep ticking it while the interval's consumer runs.
/// `Interval` is `!Unpin` and must be pinned before polling (e.g. with
/// [`core::pin::pin!`]).
///
/// # Examples
///
/// ```rust,ignore
/// let wheel: TimerWheel<64, 4> = TimerWheel::new();
/// let mut ticks = core::pin::pin!(Interval::new(&wheel, 10));
/// loop {
///     ticks.as_mut().tick().await;
///     // do periodic work every 10 wheel ticks
/// }
/// ```
pub struct Interval<'a, const SLOTS: usize, const LEVELS: usize> {
    wheel: &'a TimerWheel<SLOTS, LEVELS>,
    /// The re-armed sleep backing the next tick.
    sleep: Sleep<'a, SLOTS, LEVELS>,
    period: u64,
    behavior: MissedTickBehavior,
    /// `Sleep` is `!Unpin`; keep the whole interval `!Unpin`.
    _pin: PhantomPinned,
}

impl<'a, const SLOTS: usize, const LEVELS: usize> Interval<'a, SLOTS, LEVELS> {
    /// Create an interval that fires every `period` ticks, with the first
    /// tick firing `period` ticks from the current wheel time.
    ///
    /// # Panics
    /// Panics if `period == 0` (an interval that always fires would starve
    /// the executor).
    pub fn new(wheel: &'a TimerWheel<SLOTS, LEVELS>, period: u64) -> Self {
        assert!(period > 0, "Interval::new: period must be non-zero");
        let now = wheel.now();
        Self {
            wheel,
            sleep: Sleep::new(wheel, now + period),
            period,
            behavior: MissedTickBehavior::Skip,
            _pin: PhantomPinned,
        }
    }

    /// Set the missed-tick behaviour (builder style).
    #[must_use]
    pub fn missed_tick_behavior(mut self, behavior: MissedTickBehavior) -> Self {
        self.behavior = behavior;
        self
    }

    /// The current missed-tick behaviour.
    #[must_use]
    pub fn behavior(&self) -> MissedTickBehavior {
        self.behavior
    }

    /// The absolute wheel tick of the next tick.
    #[must_use]
    pub fn next_deadline(&self) -> u64 {
        self.sleep.deadline()
    }

    /// Wait for the next tick.
    ///
    /// Returns `Poll::Ready(())` every `period` ticks (adjusted by this
    /// interval's [`MissedTickBehavior`]) and re-arms for the following
    /// period.
    pub fn poll_tick(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        // SAFETY: structural pin projection; `Interval` is `!Unpin` so field
        // addresses are stable while pinned, and we never move fields out.
        let this = unsafe { self.get_unchecked_mut() };

        // SAFETY: `sleep` is structurally pinned through `self`.
        let sleep_pin: Pin<&mut Sleep<'a, SLOTS, LEVELS>> =
            unsafe { Pin::new_unchecked(&mut this.sleep) };

        match sleep_pin.poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(()) => {
                let old = this.sleep.deadline();
                let now = this.wheel.now();
                let next = this.behavior.next_deadline(old, this.period, now);
                // Burst deliberately re-arms in the past; `Sleep::poll`
                // resolves such deadlines immediately without registering.
                this.sleep.reset(next);
                Poll::Ready(())
            }
        }
    }

    /// Returns a future that resolves on the next interval tick.
    pub fn tick(self: Pin<&mut Self>) -> Tick<'_, 'a, SLOTS, LEVELS> {
        Tick { interval: self }
    }
}

/// Future returned by [`Interval::tick`].
pub struct Tick<'i, 'a, const SLOTS: usize, const LEVELS: usize> {
    interval: Pin<&'i mut Interval<'a, SLOTS, LEVELS>>,
}

impl<'a, const SLOTS: usize, const LEVELS: usize> Future for Tick<'_, 'a, SLOTS, LEVELS> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        // SAFETY: `Tick` is a transparent wrapper; project to `interval`.
        // `Tick` itself holds its target pre-pinned and is never moved out of.
        let this = unsafe { self.get_unchecked_mut() };
        this.interval.as_mut().poll_tick(cx)
    }
}

impl<const SLOTS: usize, const LEVELS: usize> core::fmt::Debug for Interval<'_, SLOTS, LEVELS> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Interval")
            .field("period", &self.period)
            .field("next_deadline", &self.sleep.deadline())
            .field("behavior", &self.behavior)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wheel::TimerWheel;
    use core::task::{Context, RawWaker, RawWakerVTable, Waker};

    type Wheel = TimerWheel<8, 4>;

    fn noop_waker() -> Waker {
        static VTABLE: RawWakerVTable =
            RawWakerVTable::new(|ptr| RawWaker::new(ptr, &VTABLE), |_| {}, |_| {}, |_| {});
        // SAFETY: the vtable ignores the data pointer.
        unsafe { Waker::from_raw(RawWaker::new(core::ptr::null(), &VTABLE)) }
    }

    #[test]
    fn fires_every_period() {
        let wheel = Wheel::new();
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);

        let mut iv = core::pin::pin!(Interval::new(&wheel, 3));

        assert!(iv.as_mut().poll_tick(&mut cx).is_pending());
        assert_eq!(iv.next_deadline(), 3);

        wheel.tick_n(3);
        assert!(iv.as_mut().poll_tick(&mut cx).is_ready());
        assert_eq!(iv.next_deadline(), 6);

        assert!(iv.as_mut().poll_tick(&mut cx).is_pending());
        wheel.tick_n(3);
        assert!(iv.as_mut().poll_tick(&mut cx).is_ready());
        assert_eq!(iv.next_deadline(), 9);
    }

    #[test]
    fn skip_behavior_stays_on_grid() {
        let wheel = Wheel::new();
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);

        let mut iv = core::pin::pin!(
            Interval::new(&wheel, 3).missed_tick_behavior(MissedTickBehavior::Skip,)
        );

        // Miss two periods: tick to 8 without polling again.
        wheel.tick_n(8);
        assert!(iv.as_mut().poll_tick(&mut cx).is_ready());
        // Next deadline is the first multiple of 3 after 8 → 9.
        assert_eq!(iv.next_deadline(), 9);
        // One more tick and it fires again (no burst of catch-up ticks).
        wheel.tick();
        assert!(iv.as_mut().poll_tick(&mut cx).is_ready());
        assert_eq!(iv.next_deadline(), 12);
    }

    #[test]
    fn delay_behavior_restarts_rhythm() {
        let wheel = Wheel::new();
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);

        let mut iv = core::pin::pin!(
            Interval::new(&wheel, 3).missed_tick_behavior(MissedTickBehavior::Delay,)
        );

        wheel.tick_n(8);
        assert!(iv.as_mut().poll_tick(&mut cx).is_ready());
        assert_eq!(iv.next_deadline(), 8 + 3);
    }

    #[test]
    fn burst_behavior_catches_up() {
        let wheel = Wheel::new();
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);

        let mut iv = core::pin::pin!(
            Interval::new(&wheel, 3).missed_tick_behavior(MissedTickBehavior::Burst,)
        );

        wheel.tick_n(8);
        assert!(iv.as_mut().poll_tick(&mut cx).is_ready());
        // old deadline (3) + period → already in the past → fires again
        // immediately on the next poll.
        assert_eq!(iv.next_deadline(), 6);
        assert!(iv.as_mut().poll_tick(&mut cx).is_ready());
        assert_eq!(iv.next_deadline(), 9);
    }

    #[test]
    #[should_panic(expected = "period must be non-zero")]
    fn zero_period_panics() {
        let wheel = Wheel::new();
        let _ = Interval::new(&wheel, 0);
    }

    #[test]
    fn drop_deregisters() {
        let wheel = Wheel::new();
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);

        {
            let mut iv = core::pin::pin!(Interval::new(&wheel, 3));
            assert!(iv.as_mut().poll_tick(&mut cx).is_pending());
            assert_eq!(wheel.len(), 1);
            // `iv` (and the `Sleep` it owns) drops at the end of this block.
        }
        assert!(wheel.is_empty());
        wheel.tick_n(4); // must not panic
    }
}
