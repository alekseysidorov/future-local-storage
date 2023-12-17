use std::{
    fmt::Debug,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use imp::FutureLocalKey;
use pin_project::{pin_project, pinned_drop};

mod imp;

pub struct FutureOnceLock<T>(imp::FutureLocalKey<T>);

impl<T> FutureOnceLock<T> {
    /// Creates an empty future once lock.
    #[must_use]
    pub const fn new() -> Self {
        Self(imp::FutureLocalKey::new())
    }
}

impl<T: Send + 'static> FutureOnceLock<T> {
    /// Acquires a reference to the value in this future local storage.
    ///
    /// Unlike the [`std::thread::LocalKey::with`] this method does not initialize the value
    /// when called.
    ///
    /// # Panics
    ///
    /// This method will panic if the future local doesn't have a value set.
    #[inline]
    pub fn with<F, R>(&'static self, f: F) -> R
    where
        F: FnOnce(&T) -> R,
    {
        let value = self.0.local_key().borrow();
        f(value
            .as_ref()
            .expect("cannot access a future local value without setting it first"))
    }

    #[inline]
    pub fn take(&'static self) -> Option<T> {
        self.0.local_key().borrow_mut().take()
    }

    #[inline]
    pub fn get(&'static self) -> Option<T>
    where
        T: Copy,
    {
        *self.0.local_key().borrow()
    }

    /// Sets a value `T` as the future-local value for the future `F`.
    ///
    /// On completion of `scope`, the future-local value will be dropped.
    #[inline]
    pub fn scope<F>(&'static self, value: T, future: F) -> InstrumentedFuture<T, F>
    where
        F: Future,
    {
        future.with_scope(self, value)
    }
}

impl<T: Debug + Send + 'static> Debug for FutureOnceLock<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("FutureOnceLock").field(&self.0).finish()
    }
}

impl<T> AsRef<FutureLocalKey<T>> for FutureOnceLock<T> {
    fn as_ref(&self) -> &FutureLocalKey<T> {
        &self.0
    }
}

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

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};

    use pretty_assertions::assert_eq;

    use super::*;
    use crate::FutureLocalStorage;

    #[test]
    fn test_once_lock_trivial() {
        static LOCK: FutureOnceLock<RefCell<String>> = FutureOnceLock::new();
        LOCK.with(|x| x.replace("0".to_owned()));

        assert_eq!(LOCK.with(|x| x.borrow().clone()), "0".to_owned());
        LOCK.with(|x| x.replace("42".to_owned()));
        assert_eq!(LOCK.with(|x| x.borrow().clone()), "42".to_owned());
    }

    #[tokio::test]
    async fn test_future_once_lock() {
        static VALUE: FutureOnceLock<Cell<u64>> = FutureOnceLock::new();

        let fut_1 = async {
            for _ in 0..42 {
                VALUE.with(|x| {
                    let value = x.get();
                    x.set(value + 1);
                });
                tokio::task::yield_now().await;
            }

            VALUE.with(Cell::get)
        }
        .with_scope(&VALUE, Cell::new(0));

        let fut_2 = async { VALUE.with(Cell::get) }.with_scope(&VALUE, Cell::new(15));

        assert_eq!(fut_1.await, 42);
        assert_eq!(fut_2.await, 15);
        assert_eq!(
            tokio::spawn(async { VALUE.with(Cell::get) }.with_scope(&VALUE, Cell::new(115)))
                .await
                .unwrap(),
            115
        );
    }
}
