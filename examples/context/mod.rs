use std::{
    cell::RefCell,
    fmt::Display,
    future::Future,
    time::{Duration, Instant},
};

use future_local_storage::{FutureLocalStorage, FutureOnceLock};

static CONTEXT: FutureOnceLock<TracerContext> = FutureOnceLock::new();

#[derive(Debug)]
pub struct TracerContext {
    begin: Instant,
    traces: RefCell<Vec<String>>,
}

impl TracerContext {
    fn new() -> Self {
        Self {
            begin: Instant::now(),
            traces: RefCell::new(Vec::new()),
        }
    }

    pub fn in_scope<R, F: FnMut(&Self) -> R>(mut scope: F) -> R {
        CONTEXT.with(|tracer| scope(tracer.as_ref().unwrap()))
    }

    pub fn trace(&self, message: impl Display) {
        let log_entry = format!("{}: {message}", self.elapsed().as_secs_f32());
        self.traces.borrow_mut().push(log_entry);
    }

    pub async fn spawn<R, F>(future: F) -> (Vec<String>, R)
    where
        F: Future<Output = R>,
    {
        let mut this = Some(Self::new());
        CONTEXT.swap(&mut this);

        let result = future.attach(&CONTEXT).await;

        CONTEXT.swap(&mut this);
        (this.unwrap().traces.take(), result)
    }

    fn elapsed(&self) -> Duration {
        Instant::now().duration_since(self.begin)
    }
}
