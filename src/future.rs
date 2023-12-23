//! Future types

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
///
/// Unlike the [`ScopedFutureWithValue`] this future discards the future local value.
#[pin_project]
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

impl<T, F> Future for ScopedFutureWithValue<T, F>
where
    T: Send,
    F: Future,
{
    type Output = (T, F::Output);

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        // Swap in future local key.
        FutureLocalKey::swap(this.scope, this.value);
        // Poll the underlying future.
        let result = this.inner.poll(cx);
        // Swap future local key back.
        FutureLocalKey::swap(this.scope, this.value);

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
