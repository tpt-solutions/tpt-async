// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Deterministic testing utilities for the `tpt-async` stack.
//!
//! - [`TestDriver`] bundles a [`TimerWheel`] with a manually-advanced tick
//!   counter: register `Sleep`/`Interval`/`Timeout` futures against it, then
//!   `advance` — timeouts fire **exactly when you say so**, no wall-clock
//!   sleeps, no flakes.
//! - [`io_pair`] builds two connected [`FakeIo`] transports implementing
//!   `tpt_async_io::AsyncRead`/`AsyncWrite` for exercising protocol code
//!   without real sockets.
//!
//! # Example
//!
//! ```rust
//! use core::future::Future as _;
//! use std::pin::pin;
//! use std::task::{Context, Poll};
//!
//! use tpt_async_core::waker::noop_waker;
//! use tpt_async_test::{io_pair, TestDriver};
//! use tpt_async_timer::prelude::*;
//!
//! // Deterministic time: a sleep fires precisely when the driver advances.
//! let driver: TestDriver = TestDriver::new();
//! let mut sleep = pin!(Sleep::new(driver.wheel(), driver.now() + 10));
//!
//! let waker = noop_waker();
//! let mut cx = Context::from_waker(&waker);
//! assert!(sleep.as_mut().poll(&mut cx).is_pending());
//!
//! driver.advance(5); // halfway there
//! assert!(sleep.as_mut().poll(&mut cx).is_pending());
//!
//! driver.advance(5); // deadline reached
//! assert!(sleep.as_mut().poll(&mut cx).is_ready());
//! ```

#![cfg_attr(docsrs, feature(doc_cfg))]
#![warn(missing_docs, clippy::all)]

use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

use tpt_async_io::{AsyncRead, AsyncWrite, IoError, ReadBuf};
use tpt_async_timer::wheel::TimerWheel;

// ── TestDriver ───────────────────────────────────────────────────────────────

/// A manually-advanced timer wheel + clock for deterministic tests.
///
/// All methods take `&self` (the wheel is internally locked), so a driver
/// can be shared freely with the code under test.
///
/// The default grid is `64` slots × `4` levels — 64⁴ ≈ 16.7 million ticks of
/// range before coarse-level wrapping.  Pick other constants via
/// [`TestDriver::with_wheel`] if you need a different horizon.
pub struct TestDriver<const SLOTS: usize = 64, const LEVELS: usize = 4> {
    wheel: TimerWheel<SLOTS, LEVELS>,
}

impl<const SLOTS: usize, const LEVELS: usize> Default for TestDriver<SLOTS, LEVELS> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const SLOTS: usize, const LEVELS: usize> TestDriver<SLOTS, LEVELS> {
    /// Create a driver at tick 0.
    pub fn new() -> Self {
        Self {
            wheel: TimerWheel::new(),
        }
    }

    /// Create a driver with a custom wheel shape.
    pub fn with_wheel(wheel: TimerWheel<SLOTS, LEVELS>) -> Self {
        Self { wheel }
    }

    /// The shared wheel; register `Sleep`/`Timeout`/`Interval` against it.
    pub fn wheel(&self) -> &TimerWheel<SLOTS, LEVELS> {
        &self.wheel
    }

    /// The current tick.
    pub fn now(&self) -> u64 {
        self.wheel.now()
    }

    /// Advance time by `ticks`, firing every deadline in between (in
    /// order).  This is what makes timers deterministic: nothing fires
    /// unless you advance.
    pub fn advance(&self, ticks: u64) {
        self.wheel.tick_n(ticks);
    }

    /// Advance until `deadline` (no-op if already past it).
    pub fn advance_until(&self, deadline: u64) {
        self.wheel.advance_to(deadline);
    }
}

// ── FakeIo ───────────────────────────────────────────────────────────────────

/// A byte queue plus the waker of whoever is waiting on it.
struct Queue {
    bytes: VecDeque<u8>,
    reader_waker: Option<Waker>,
}

impl Queue {
    fn wake_reader(&mut self) {
        if let Some(w) = self.reader_waker.take() {
            w.wake();
        }
    }
}

/// One end of an in-memory connection produced by [`io_pair`].
///
/// Implements `tpt_async_io::AsyncRead` + `AsyncWrite` with real waker
/// registration, so it works under any executor.  After `shutdown` (or
/// dropping the end), the peer's reads drain buffered bytes and then return
/// EOF; writes after the peer is gone fail with `BrokenPipe`.
pub struct FakeIo {
    /// Queue this end reads from (the peer writes here).
    read_queue: Arc<Mutex<Queue>>,
    /// Queue this end writes to (the peer reads here).
    write_queue: Arc<Mutex<Queue>>,
    /// Set when the *peer* is shut down or dropped — reads then EOF after
    /// draining.
    peer_write_closed: Arc<AtomicBool>,
    /// This end's own shutdown flag, signalled to the peer on drop/shutdown.
    write_closed: Arc<AtomicBool>,
}

impl FakeIo {
    /// Bytes buffered and not yet read from this end.
    pub fn pending_bytes(&self) -> usize {
        self.read_queue
            .lock()
            .expect("fake io poisoned")
            .bytes
            .len()
    }

    /// Whether the peer has shut down / dropped.
    pub fn peer_gone(&self) -> bool {
        self.peer_write_closed.load(Ordering::Acquire)
    }

    /// Signal EOF to the peer (its reads drain, then return zero bytes).
    pub fn shutdown(&mut self) {
        eprintln!("DEBUG FakeIo::shutdown called");
        self.write_closed.store(true, Ordering::Release);
        self.write_queue
            .lock()
            .expect("fake io poisoned")
            .wake_reader();
    }
}

impl Drop for FakeIo {
    fn drop(&mut self) {
        eprintln!("DEBUG FakeIo::drop (write_closed flag -> true)");
        self.write_closed.store(true, Ordering::Release);
        self.write_queue
            .lock()
            .expect("fake io poisoned")
            .wake_reader();
    }
}

impl AsyncRead for FakeIo {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<Result<(), IoError>> {
        if buf.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }
        let mut queue = self.read_queue.lock().expect("fake io poisoned");
        let n = queue.bytes.len().min(buf.remaining());
        if n == 0 {
            if self.peer_write_closed.load(Ordering::Acquire) {
                return Poll::Ready(Ok(())); // EOF after full drain
            }
            queue.reader_waker = Some(cx.waker().clone());
            return Poll::Pending;
        }
        for _ in 0..n {
            if let Some(b) = queue.bytes.pop_front() {
                buf.unfilled()[0] = b;
                buf.advance(1);
            }
        }
        Poll::Ready(Ok(()))
    }
}

impl AsyncWrite for FakeIo {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, IoError>> {
        if self.peer_gone() {
            return Poll::Ready(Err(IoError::new(
                std::io::ErrorKind::BrokenPipe,
                "fake io: peer has shut down",
            )));
        }
        if buf.is_empty() {
            return Poll::Ready(Ok(0));
        }
        let mut queue = self.write_queue.lock().expect("fake io poisoned");
        queue.bytes.extend(buf.iter().copied());
        queue.wake_reader();
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), IoError>> {
        // Writes are already buffered for the peer; nothing to do.
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), IoError>> {
        self.write_closed.store(true, Ordering::Release);
        self.write_queue
            .lock()
            .expect("fake io poisoned")
            .wake_reader();
        Poll::Ready(Ok(()))
    }
}

/// Build a connected pair of in-memory transports.
///
/// Bytes written to one end become readable at the other, with waker-based
/// wakeup in both directions.  Dropping (or `shutdown`) one end drains
/// cleanly to EOF at the other.
pub fn io_pair() -> (FakeIo, FakeIo) {
    let q_ab = Arc::new(Mutex::new(Queue {
        bytes: VecDeque::new(),
        reader_waker: None,
    })); // end A writes, end B reads
    let q_ba = Arc::new(Mutex::new(Queue {
        bytes: VecDeque::new(),
        reader_waker: None,
    })); // end B writes, end A reads
    let alive_a = Arc::new(AtomicBool::new(false)); // end A not shut down
    let alive_b = Arc::new(AtomicBool::new(false)); // end B not shut down

    let end_a = FakeIo {
        read_queue: Arc::clone(&q_ba),
        write_queue: Arc::clone(&q_ab),
        peer_write_closed: Arc::clone(&alive_b),
        write_closed: Arc::clone(&alive_a),
    };
    (
        end_a,
        FakeIo {
            read_queue: Arc::clone(&q_ab),
            write_queue: Arc::clone(&q_ba),
            peer_write_closed: Arc::clone(&alive_a),
            write_closed: Arc::clone(&alive_b),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::pin::pin;

    use tpt_async_core::waker::noop_waker;
    use tpt_async_timer::prelude::*;

    #[test]
    fn sleep_fires_exactly_on_advance() {
        let driver: TestDriver = TestDriver::new();
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);

        let mut sleep = pin!(Sleep::new(driver.wheel(), driver.now() + 10));
        assert!(sleep.as_mut().poll(&mut cx).is_pending());

        driver.advance(9);
        assert!(
            sleep.as_mut().poll(&mut cx).is_pending(),
            "fired one tick early"
        );

        driver.advance(1);
        assert!(
            sleep.as_mut().poll(&mut cx).is_ready(),
            "did not fire at deadline"
        );
    }

    #[test]
    fn advance_fires_multiple_deadlines() {
        let driver: TestDriver = TestDriver::new();
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);

        let mut a = pin!(Sleep::new(driver.wheel(), 3));
        let mut b = pin!(Sleep::new(driver.wheel(), 7));
        assert!(a.as_mut().poll(&mut cx).is_pending());
        assert!(b.as_mut().poll(&mut cx).is_pending());

        driver.advance(10);
        assert!(a.as_mut().poll(&mut cx).is_ready());
        assert!(b.as_mut().poll(&mut cx).is_ready());
    }

    fn spin<F: Future>(future: F) -> F::Output {
        fn noop() -> Waker {
            static VTABLE: std::task::RawWakerVTable = std::task::RawWakerVTable::new(
                |p| std::task::RawWaker::new(p, &VTABLE),
                |_| {},
                |_| {},
                |_| {},
            );
            // SAFETY: the vtable ignores the data pointer.
            unsafe { Waker::from_raw(std::task::RawWaker::new(core::ptr::null(), &VTABLE)) }
        }
        let mut fut = pin!(future);
        let waker = noop();
        let mut cx = Context::from_waker(&waker);
        loop {
            if let Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
                return v;
            }
            std::hint::spin_loop();
        }
    }

    #[test]
    fn fake_pair_ext_roundtrip_via_spin() {
        let (mut a, mut b) = io_pair();

        spin(async {
            use tpt_async_io::AsyncWriteExt as _;
            a.write_all(b"ping").await.expect("write");
            a.shutdown();
        });

        let received = spin(async {
            use tpt_async_io::AsyncReadExt as _;
            let mut out = Vec::new();
            b.read_to_end(&mut out).await.expect("read_to_end");
            out
        });
        assert_eq!(received, b"ping");
    }

    #[test]
    fn fake_pair_write_after_peer_gone_fails() {
        let (mut a, b) = io_pair();
        drop(b);
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        assert!(
            std::future::poll_fn(|cx| Pin::new(&mut a).poll_write(cx, b"x"))
                .await_spin()
                .is_err()
        );
    }

    #[test]
    fn fake_pair_shutdown_gives_peer_eof() {
        let (mut a, mut b) = io_pair();
        a.shutdown();
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        let mut buf = [0u8; 4];
        let mut rb = ReadBuf::new(&mut buf);
        std::future::poll_fn(|cx| Pin::new(&mut b).poll_read(cx, &mut rb))
            .await_spin()
            .expect("eof read");
        assert!(rb.filled().is_empty(), "EOF: zero bytes filled");
    }

    trait AwaitSpin: Future {
        fn await_spin(self) -> Self::Output
        where
            Self: Sized,
        {
            fn noop() -> Waker {
                static VTABLE: std::task::RawWakerVTable = std::task::RawWakerVTable::new(
                    |p| std::task::RawWaker::new(p, &VTABLE),
                    |_| {},
                    |_| {},
                    |_| {},
                );
                // SAFETY: the vtable ignores the data pointer.
                unsafe { Waker::from_raw(std::task::RawWaker::new(core::ptr::null(), &VTABLE)) }
            }
            let mut fut = pin!(self);
            let waker = noop();
            let mut cx = Context::from_waker(&waker);
            loop {
                if let Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
                    return v;
                }
                std::hint::spin_loop();
            }
        }
    }
    impl<F: Future> AwaitSpin for F {}
}
