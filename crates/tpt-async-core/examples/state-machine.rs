// Copyright TPT Solutions
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Zero-cost declarative state machines, no_std, no allocation.

use tpt_async_core::state_machine;

state_machine! {
    /// TCP-ish connection lifecycle.
    pub Conn, ConnEvent {
        states: [Closed, Listening, Established, ClosedByPeer],
        events: [Listen, Accept, Close, Reset],

        Closed -- Listen --> Listening,
        Listening -- Accept --> Established,
        Established -- Close --> Closed,
        Listening -- Reset --> ClosedByPeer,
    }
}

fn main() {
    let state = Conn::Closed;

    let state = state
        .transition(ConnEvent::Listen)
        .expect("Closed --Listen--> Listening is in the table");
    println!("after Listen: {state:?}");

    let state = state
        .transition(ConnEvent::Accept)
        .expect("Listening --Accept--> Established is in the table");
    println!("after Accept: {state:?}");

    // Invalid transitions return None instead of panicking:
    assert_eq!(state.transition(ConnEvent::Listen), None);

    println!("all states: {:?}", Conn::ALL_STATES);
}
