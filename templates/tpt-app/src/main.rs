use std::time::Duration;

use tpt_async::prelude::*;

#[tpt_async::main]
async fn main() {
    println!("{{project-name}} — running on tpt-async");

    // The std timer driver makes sleep/timeout just work:
    sleep(Duration::from_millis(100)).await;
    println!("slept 100 ms on the tpt-async timer");

    let result = timeout(Duration::from_secs(1), async {
        // your work here
        42
    })
    .await
    .expect("operation timed out");
    println!("timeout result: {result}");
}
