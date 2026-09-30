//! Future types.

use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use pin_project::{pin_project, pinned_drop};

use crate::{FutureLocalStorage, imp::FutureLocalKey};

impl<F: Future> FutureLocalStorage for F {
    fn with_scope<T, S>(self, scope: &'static S, value: T) -> ScopedFutureWithValue<T, Self>
    where
        T: Send,
        S: AsRef<FutureLocalKey<T>>,
    {
        let scope = scope.as_ref();
        ScopedFutureWithValue {
            inner: self,
            scope,
            value: Some(value),
        }
    }
}

/// A [`Future`] that sets a value `T` of a future local for the future `F` during its execution.
/// Unlike the [`ScopedFutureWithValue`] this future discards the future local value.
#[pin_project]
#[derive(Debug)]
pub struct ScopedFuture<T, F>(#[pin] ScopedFutureWithValue<T, F>)
where
    T: Send + 'static,
    F: Future;

impl<T, F> Future for ScopedFuture<T, F>
where
    T: Send,
    F: Future,
{
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.project().0.poll(cx).map(|(_value, result)| result)
    }
}

impl<T, F> ScopedFutureWithValue<T, F>
where
    T: Send,
    F: Future,
{
    /// Discards the future local value from the future output.
    pub fn discard_value(self) -> ScopedFuture<T, F> {
        ScopedFuture(self)
    }
}

/// A [`Future`] that sets a value `T` of a future local for the future `F` during its execution.
///
/// This future also returns a future local value after execution.
#[pin_project(PinnedDrop)]
#[derive(Debug)]
pub struct ScopedFutureWithValue<T, F>
where
    T: Send + 'static,
    F: Future,
{
    // TODO Implement manually drop to provide scope access to the future Drop.
    #[pin]
    inner: F,
    scope: &'static FutureLocalKey<T>,
    value: Option<T>,
}

#[pinned_drop]
impl<T, F> PinnedDrop for ScopedFutureWithValue<T, F>
where
    F: Future,
    T: Send + 'static,
{
    fn drop(self: Pin<&mut Self>) {}
}

struct FutureLocalGuard<'a, T: Send + 'static> {
    scope: &'static FutureLocalKey<T>,
    value: &'a mut Option<T>,
    active: bool,
}

impl<'a, T: Send + 'static> FutureLocalGuard<'a, T> {
    fn enter(scope: &'static FutureLocalKey<T>, value: &'a mut Option<T>) -> Self {
        FutureLocalKey::swap(scope, value);
        Self {
            scope,
            value,
            active: true,
        }
    }

    fn exit(&mut self) {
        if self.active {
            FutureLocalKey::swap(self.scope, self.value);
            self.active = false;
        }
    }
}

impl<T: Send + 'static> Drop for FutureLocalGuard<'_, T> {
    fn drop(&mut self) {
        self.exit();
    }
}

impl<T, F> Future for ScopedFutureWithValue<T, F>
where
    T: Send,
    F: Future,
{
    type Output = (T, F::Output);

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let mut guard = FutureLocalGuard::enter(this.scope, this.value);
        // Poll the underlying future.
        let result = this.inner.poll(cx);
        guard.exit();
        drop(guard);

        let result = std::task::ready!(result);
        // Take the scoped value to return it back to the future caller.
        let value = this.value.take().unwrap();
        Poll::Ready((value, result))
    }
}

impl<T, F> From<ScopedFutureWithValue<T, F>> for ScopedFuture<T, F>
where
    T: Send,
    F: Future,
{
    fn from(value: ScopedFutureWithValue<T, F>) -> Self {
        Self(value)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        future::{Future, pending},
        panic::AssertUnwindSafe,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        task::{Context, Poll},
    };

    use futures_util::task::noop_waker;

    use super::*;
    use crate::FutureOnceCell;

    struct DropCounter(Arc<AtomicUsize>);

    impl Drop for DropCounter {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    // Cancellation and cleanup.
    #[test]
    fn dropping_pending_future_drops_scoped_value() {
        static CELL: FutureOnceCell<DropCounter> = FutureOnceCell::new();
        let drops = Arc::new(AtomicUsize::new(0));
        let value = DropCounter(Arc::clone(&drops));
        let mut future = Box::pin(pending::<()>().with_scope(&CELL, value));
        let waker = noop_waker();
        let mut context = Context::from_waker(&waker);

        assert!(matches!(future.as_mut().poll(&mut context), Poll::Pending));
        drop(future);

        assert_eq!(drops.load(Ordering::SeqCst), 1);
        // Cancellation must leave the thread-local slot in its previous state.
        assert!(CELL.0.local_key().borrow().is_none());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cancelled_future_does_not_leak_value_to_the_next_future() {
        static CELL: FutureOnceCell<u32> = FutureOnceCell::new();
        let mut future = Box::pin(pending::<()>().with_scope(&CELL, 7));
        let waker = noop_waker();
        let mut context = Context::from_waker(&waker);

        assert!(matches!(future.as_mut().poll(&mut context), Poll::Pending));
        drop(future);

        // A later future must not observe state from the cancelled future.
        assert_eq!(CELL.0.local_key().borrow().as_ref(), None);
        let (value, ()) = CELL.scope(9, async {}).await;
        assert_eq!(value, 9);
    }

    // Panic safety.
    #[test]
    fn panic_during_poll_restores_previous_value() {
        static CELL: FutureOnceCell<usize> = FutureOnceCell::new();
        let mut future = Box::pin(
            async {
                panic!("poll failed");
            }
            .with_scope(&CELL, 42),
        );
        let waker = noop_waker();
        let mut context = Context::from_waker(&waker);

        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let _ = future.as_mut().poll(&mut context);
        }));

        assert!(result.is_err());
        assert_eq!(CELL.0.local_key().borrow().as_ref(), None);
    }

    // Nested scopes.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn nested_scopes_restore_the_outer_value() {
        static CELL: FutureOnceCell<u32> = FutureOnceCell::new();

        let (outer, ()) = CELL
            .scope(1, async {
                assert_eq!(CELL.get(), 1);

                let (inner, ()) = CELL
                    .scope(2, async {
                        assert_eq!(CELL.get(), 2);
                        tokio::task::yield_now().await;
                        assert_eq!(CELL.get(), 2);
                    })
                    .await;

                assert_eq!(inner, 2);
                assert_eq!(CELL.get(), 1);
            })
            .await;

        assert_eq!(outer, 1);
        assert_eq!(CELL.0.local_key().borrow().as_ref(), None);
    }
}
