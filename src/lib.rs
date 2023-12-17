use std::{
    future::Future,
    mem::ManuallyDrop,
    pin::Pin,
    task::{Context, Poll},
};

use imp::FutureLocalKey;
#[cfg(feature = "unstable")]
pub use lazy_lock::FutureLazyLock;
pub use once_lock::FutureOnceLock;
use pin_project::{pin_project, pinned_drop};

mod imp;
#[cfg(feature = "unstable")]
mod lazy_lock;
mod once_lock;

/// Attaches future local storage to a [`Future`].
///
/// Extension trait allowing futures to have their own static variables.
pub trait FutureLocalStorage: Future + Sized + private::Sealed {
    /// Instruments this future with the provided static value.
    ///
    /// Each future instance will have its own state of the attached value.
    fn attach<T, S>(self, lock: &'static S) -> InstrumentedFuture<T, Self>
    where
        T: Send,
        S: AsRef<FutureLocalKey<T>>;
}

impl<F: Future> FutureLocalStorage for F {
    fn attach<T, S>(self, storage: &'static S) -> InstrumentedFuture<T, Self>
    where
        T: Send,
        S: AsRef<FutureLocalKey<T>>,
    {
        let storage = storage.as_ref();
        let mut future = InstrumentedFuture {
            inner: self,
            storage,
            stored_value: None,
        };
        // Take a value from a future local key in order to set it again when future will
        // be polled.
        FutureLocalKey::swap(storage, &mut future.stored_value);
        future
    }
}

/// A [`Future`] that has been instrumented with a [`FutureLocalStorage`].
#[pin_project(PinnedDrop)]
pub struct InstrumentedFuture<T, F>
where
    T: Send + 'static,
    F: Future,
{
    // TODO add support to instrument Drop.
    #[pin]
    inner: F,
    storage: &'static FutureLocalKey<T>,
    stored_value: Option<T>,
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
        FutureLocalKey::swap(this.storage, this.stored_value);
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
        FutureLocalKey::swap(this.storage, this.stored_value);
        // Poll the underlying future.
        let result = this.inner.poll(cx);
        // Swap future local key back.
        FutureLocalKey::swap(this.storage, this.stored_value);

        result
    }
}

mod private {
    use std::future::Future;

    pub trait Sealed {}

    impl<F: Future> Sealed for F {}
}
