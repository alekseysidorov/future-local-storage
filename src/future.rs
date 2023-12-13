//! Future local storage extensions for the [`std::future::Future`].

use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use pin_project_lite::pin_project;

use crate::{imp::FutureLocalKey, FutureLazyLock, FutureOnceLock};

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

impl<T> AsRef<FutureLocalKey<T>> for FutureOnceLock<T> {
    fn as_ref(&self) -> &FutureLocalKey<T> {
        &self.0
    }
}

impl<T> AsRef<FutureLocalKey<T>> for FutureLazyLock<T> {
    fn as_ref(&self) -> &FutureLocalKey<T> {
        &self.inner
    }
}

pub trait Storage<T>: private::Sealed {
    fn insturment<F>(&'static self, future: F) -> InstrumentedFuture<T, F>
    where
        F: Future;
}

mod private {
    use std::future::Future;

    pub trait Sealed {}

    impl<F: Future> Sealed for F {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_future_once_lock() {
        static VALUE: FutureOnceLock<u64> = FutureOnceLock::new();

        let fut_1 = async {
            for _ in 0..42 {
                let j = VALUE.with(|x| x.unwrap_or_default());
                VALUE.replace(j + 1);
                tokio::task::yield_now().await;
            }

            VALUE.get().unwrap()
        }
        .attach(&VALUE);

        let fut_2 = async {
            VALUE.replace(15);
            VALUE.get().unwrap()
        }
        .attach(&VALUE);

        assert_eq!(fut_1.await, 42);
        assert_eq!(fut_2.await, 15);
        assert_eq!(
            tokio::spawn(
                async {
                    VALUE.replace(115);
                    VALUE.get().unwrap()
                }
                .attach(&VALUE)
            )
            .await
            .unwrap(),
            115
        );
    }

    #[tokio::test]
    async fn test_future_lazy() {
        static VALUE: FutureLazyLock<i32> = FutureLazyLock::new(|| -1);

        let fut_1 = async {
            for _ in 0..42 {
                let j = VALUE.with(|x| *x);
                VALUE.replace(j + 1);
                tokio::task::yield_now().await;
            }

            VALUE.get()
        }
        .attach(&VALUE);

        let fut_2 = async {
            VALUE.replace(15);
            tokio::task::yield_now().await;
            VALUE.get()
        }
        .attach(&VALUE);

        assert_eq!(fut_1.await, 41);
        assert_eq!(fut_2.await, 15);
        assert_eq!(
            tokio::spawn(
                async {
                    VALUE.replace(115);
                    tokio::task::yield_now().await;
                    VALUE.get()
                }
                .attach(&VALUE)
            )
            .await
            .unwrap(),
            115
        );
    }
}
