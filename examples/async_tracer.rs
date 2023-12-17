use std::{cell::RefCell, fmt::Display, time::Duration};

use context::TracerContext;

mod context;

async fn second_long_computation(a: u64) -> u64 {
    TracerContext::in_scope(|context| {
        context.trace(format!("[Entered] `second_long_computation` with params: a={a}"));
    });
    
    tokio::time::sleep(Duration::from_millis(100)).await;

    TracerContext::in_scope(|context| {
        context.trace("[Exited] `second_long_computation`");
    });
    a * 32
}

async fn first_computation(a: u64) -> u64 {
    TracerContext::in_scope(|context| {
        context.trace(format!("[Entered] `first_computation` with params: a={a}"));
    });

    tokio::task::yield_now().await;

    TracerContext::in_scope(|context| {
        context.trace("[Exited] `first_computation`");
    });
    a + 2
}

async fn some_method(name: impl Display, mut a: u64) -> u64 {
    TracerContext::in_scope(|context| {
        context.trace(format!("[Entered] `some_method` with params: name={name}, a={a}"));
    });

    a = first_computation(a).await;
    a = second_long_computation(a).await;

    TracerContext::in_scope(|context| {
        context.trace("[Exited] `some_method`");
    });
    a
}

#[tokio::main]
async fn main() {
    let (traces, answer) = TracerContext::spawn(some_method("first", 0)).await;

    println!("Computation answer is {answer}");
    println!("Captured traces:");
    for trace in traces {
        println!("\t{trace}");
    }
}
