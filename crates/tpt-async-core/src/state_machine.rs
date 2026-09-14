// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! A zero-cost declarative state-machine generator: see [`state_machine!`](crate::state_machine!).
//!
//! Part of the crate's design goals (see `spec.txt`): protocol code on
//! embedded targets wants explicit, allocation-free state machines.  The
//! macro generates two plain enums (states, events) and a `match`-based
//! transition function — no dyn dispatch, no heap, fully `const`-friendly.

/// Generate a zero-cost state machine.
///
/// # Shape
///
/// ```text
/// state_machine! {
///     $(#[$machine_meta:meta])*
///     $vis machine Ident, EventIdent {
///         states: [A, B, …],
///         events: [Ev1, Ev2, …],
///         A -- Ev1 --> B,
///         B -- Ev2 --> A,
///         …
///     }
/// }
/// ```
///
/// # Generated items
///
/// - `$vis enum $machine` — the states.
/// - `$vis enum $event` — the event alphabet.
/// - `$machine::transition(self, event) -> Option<$machine>` — the transition
///   table as a `match`; `None` for uncovered (state, event) pairs.
/// - `$machine::ALL_STATES` — a `const` array for exhaustiveness checks.
///
/// # Example
///
/// ```
/// use tpt_async_core::state_machine;
///
/// state_machine! {
///     /// A tiny TCP-ish protocol machine.
///     pub Conn, ConnEvent {
///         states: [Closed, Listening, Established],
///         events: [Start, Accept, Close],
///
///         Closed -- Start --> Listening,
///         Listening -- Accept --> Established,
///         Established -- Close --> Closed,
///     }
/// }
///
/// let mut s = Conn::Closed;
/// s = s.transition(ConnEvent::Start).unwrap();
/// assert_eq!(s, Conn::Listening);
/// s = s.transition(ConnEvent::Accept).unwrap();
/// assert_eq!(s, Conn::Established);
/// assert!(s.transition(ConnEvent::Start).is_none(), "invalid transitions rejected");
/// ```
#[macro_export]
macro_rules! state_machine {
    (
        $(#[$machine_meta:meta])*
        $vis:vis $machine:ident, $event:ident {
            states: [$($state:ident),* $(,)?],
            events: [$($evt:ident),* $(,)?],
            $($from:ident -- $on:ident --> $to:ident),* $(,)?
        }
    ) => {
        $(#[$machine_meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[allow(missing_docs)]
        $vis enum $machine {
            $($state),*
        }

        impl $machine {
            /// Every state, in declaration order.
            $vis const ALL_STATES: &'static [Self] = &[$(Self::$state),*];

            /// Apply `event`, returning the next state, or `None` when the
            /// (state, event) pair is not in the transition table.
            #[must_use]
            $vis const fn transition(
                self,
                event: $event,
            ) -> Option<Self> {
                match (self, event) {
                    $(($machine::$from, $event::$on) => Some($machine::$to),)*
                    _ => None,
                }
            }
        }

        /// The event alphabet of the [`state_machine!`](crate::state_machine)
        /// `$machine`.
        #[doc(hidden)]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[allow(missing_docs)]
        $vis enum $event {
            $($evt),*
        }

        impl $event {
            /// Every event, in declaration order.
            $vis const ALL_EVENTS: &'static [Self] = &[$(Self::$evt),*];
        }
    };
}
