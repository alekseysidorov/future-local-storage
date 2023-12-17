use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use imp::FutureLocalKey;
pub use once_lock::FutureOnceLock;
use pin_project::{pin_project, pinned_drop};

mod imp;
mod once_lock;

/// Attaches future local storage to a [`Future`].
///
/// Extension trait allowing futures to have their own static variables.
pub trait FutureLocalStorage: Future + Sized + private::Sealed {
    /// Instruments this future in scope of the provided static value.
    ///
    /// Each future instance will have its own state of the attached value.
    fn with_scope<T, S>(self, scope: &'static S, value: T) -> InstrumentedFuture<T, Self>
    where
        T: Send,
        S: AsRef<FutureLocalKey<T>>;
}

impl<F: Future> FutureLocalStorage for F {
    fn with_scope<T, S>(self, scope: &'static S, value: T) -> InstrumentedFuture<T, Self>
    where
        T: Send,
        S: AsRef<FutureLocalKey<T>>,
    {
        let scope = scope.as_ref();
        InstrumentedFuture {
            inner: self,
            scope,
            value: Some(value),
        }
    }
}

/// A [`Future`] that has been instrumented with a [`FutureLocalStorage`].
#[pin_project(PinnedDrop)]
pub struct InstrumentedFuture<T, F>
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
impl<T, F> PinnedDrop for InstrumentedFuture<T, F>
where
    F: Future,
    T: Send + 'static,
{
    fn drop(self: Pin<&mut Self>) {
        let this = self.project();
        // TODO
        FutureLocalKey::swap(this.scope, this.value);
    }
}

impl<T, F> Future for InstrumentedFuture<T, F>
where
    T: Send,
    F: Future,
{
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        // Swap in future local key.
        FutureLocalKey::swap(this.scope, this.value);
        // Poll the underlying future.
        let result = this.inner.poll(cx);
        // Swap future local key back.
        FutureLocalKey::swap(this.scope, this.value);

        result
    }
}

mod private {
    use std::future::Future;

    pub trait Sealed {}

    impl<F: Future> Sealed for F {}
}
