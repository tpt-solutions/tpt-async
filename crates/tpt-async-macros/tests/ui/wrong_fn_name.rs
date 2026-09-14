use tpt_async::prelude::*;

#[tpt_async::main]
async fn not_main() {
    println!("wrong name");
}

fn main() {}
