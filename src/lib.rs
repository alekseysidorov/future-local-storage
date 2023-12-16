use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use imp::FutureLocalKey;
#[cfg(feature = "unstable")]
pub use lazy_lock::FutureLazyLock;
pub use once_lock::FutureOnceLock;
use pin_project_lite::pin_project;

mod imp;
#[cfg(feature = "unstable")]
mod lazy_lock;
mod once_lock;

/// Attaches future local storage to a [`Future`].
///
/// Extension trait allowing futures to have their own static variables.
pub trait FutureLocalStorage: Sized + private::Sealed {
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
        InstrumentedFuture {
            inner: self,
            storage: storage.as_ref(),
            stored_value: None,
        }
    }
}

pin_project! {
    /// A [`Future`] that has been instrumented with a [`FutureLocalStorage`].
    pub struct InstrumentedFuture<T: 'static, F> {
        // TODO add support to instrument Drop.
        #[pin]
        inner: F,
        storage: & 'static FutureLocalKey<T>,
        stored_value: Option<T>,
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
        std::mem::swap(
            this.stored_value,
            &mut *this.storage.local_key().borrow_mut(),
        );
        // Poll the underlying future.
        let result = this.inner.poll(cx);
        // Swap future local key back.
        std::mem::swap(
            this.stored_value,
            &mut *this.storage.local_key().borrow_mut(),
        );

        result
    }
}

mod private {
    use std::future::Future;

    pub trait Sealed {}

    impl<F: Future> Sealed for F {}
}
