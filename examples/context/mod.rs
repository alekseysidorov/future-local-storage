use std::{
    cell::RefCell,
    fmt::Display,
    future::Future,
    time::{Duration, Instant},
};

use future_local_storage::{FutureLocalStorage, FutureOnceLock};

static CONTEXT: FutureOnceLock<TracerContext> = FutureOnceLock::new();

#[derive(Debug)]
pub struct TraceEntry {
    pub duration_since: Duration,
    pub event: String,
    pub message: String,
}

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

#[derive(Debug)]
pub struct TracerContext {
    begin: Instant,
    traces: RefCell<Vec<TraceEntry>>,
}

impl TracerContext {
    pub fn on_enter(message: impl Display) {
        Self::with(|tracer| tracer.add_entry("entered", message));
    }

    pub fn on_exit(message: impl Display) {
        Self::with(|tracer| tracer.add_entry("exited", message));
    }

    pub async fn in_scope<R, F>(future: F) -> (Vec<TraceEntry>, R)
    where
        F: Future<Output = R>,
    {
        let mut this = Some(Self::new());
        CONTEXT.swap(&mut this);

        let result = future.with_scope(&CONTEXT).await;

        CONTEXT.swap(&mut this);
        (this.unwrap().traces.take(), result)
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
        CONTEXT.with(|tracer| scope(tracer.as_ref().unwrap()))
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
