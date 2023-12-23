use std::{
    cell::RefCell,
    fmt::Display,
    future::Future,
    time::{Duration, Instant},
};

use future_local_storage::{FutureLocalStorage, FutureOnceCell};

/// Tracing span context.
static CONTEXT: FutureOnceCell<TracerContext> = FutureOnceCell::new();

#[derive(Debug)]
pub struct TracerContext {
    begin: Instant,
    traces: RefCell<Vec<TraceEntry>>,
}

impl TracerContext {
    /// Adds an "entered" event to the trace.
    pub fn on_enter(message: impl Display) {
        Self::with(|tracer| tracer.add_entry("entered", message));
    }

    /// Adds an "exit" event to the trace.
    pub fn on_exit(message: impl Display) {
        Self::with(|tracer| tracer.add_entry("exited", message));
    }

    /// Each future has its own tracing context in which it is executed, and after execution
    /// is complete, all events are returned along with the future output.
    pub async fn in_scope<R, F>(future: F) -> (Vec<TraceEntry>, R)
    where
        F: Future<Output = R>,
    {
        let (this, result) = future.with_scope(&CONTEXT, Self::new()).await;
        (this.traces.take(), result)
    }

    fn new() -> Self {
        let tracer = Self {
            begin: Instant::now(),
            traces: RefCell::new(Vec::new()),
        };
        tracer.add_entry("created", "a new async tracer started");
        tracer
    }

    fn with<R, F: FnOnce(&Self) -> R>(scope: F) -> R {
        CONTEXT.with(|tracer| scope(tracer))
    }

    fn add_entry(&self, event: impl Display, message: impl Display) {
        self.traces.borrow_mut().push(TraceEntry {
            duration_since: self.elapsed(),
            event: event.to_string(),
            message: message.to_string(),
        });
    }

    fn elapsed(&self) -> Duration {
        Instant::now().duration_since(self.begin)
    }
}

#[derive(Debug)]
pub struct TraceEntry {
    pub duration_since: Duration,
    pub event: String,
    pub message: String,
}
