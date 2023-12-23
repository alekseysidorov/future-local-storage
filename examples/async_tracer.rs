//! A toy async tracer example.

use std::{fmt::Display, time::Duration};

use context::{TraceEntry, TracerContext};

mod context;

impl Display for TraceEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:>8.2}ms: {:>12} {}",
            self.duration_since.as_secs_f64() * 1_000_f64,
            self.event,
            self.message
        )
    }
}

async fn second_long_computation(a: u64) -> u64 {
    TracerContext::on_enter(format!("`second_long_computation` with params: a={a}"));

    // Imitate a CPU-intensive computation.
    for _ in 0..10 {
        std::thread::sleep(Duration::from_millis(10));
        tokio::task::yield_now().await;
    }

    TracerContext::on_exit("`second_long_computation`");
    a * 32
}

async fn first_computation(a: u64) -> u64 {
    TracerContext::on_enter(format!("`first_computation` with params: a={a}"));

    tokio::task::yield_now().await;

    TracerContext::on_exit("`first_computation`");
    a + 2
}

async fn some_method(name: impl Display, mut a: u64) -> u64 {
    tokio::task::yield_now().await;
    TracerContext::on_enter(format!("`some_method` with params: name='{name}', a={a}"));

    a = first_computation(a).await;
    a = second_long_computation(a).await;

    TracerContext::on_exit("`some_method`");
    a
}

#[tokio::main]
async fn main() {
    // Spawn a lot of async computations in the multithreading runtime.
    let handles: Vec<_> = [
        (1, "lorem"),
        (2, "ipsum"),
        (3, "dolor"),
        (4, "sit"),
        (5, "amet"),
        (6, "consectetur"),
        (7, "adipiscing"),
        (8, "elit"),
        (9, "mauris"),
        (10, "at consequat"),
        (11, "dui"),
        (12, "vel"),
        (13, "convallis"),
        (14, "purus"),
    ]
    .into_iter()
    .map(|(i, method)| tokio::spawn(TracerContext::in_scope(some_method(method, i))))
    .collect();

    // Wait for them for complete.
    let results = futures_util::future::join_all(handles).await;
    for result in results {
        let (trace, answer) = result.expect("unable to join future");

        println!("Computation finished with answer: {answer}");
        println!("Captured traces:");
        for entry in trace {
            println!("{entry}");
        }
        println!();
    }
}
