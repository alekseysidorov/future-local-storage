//! # Overview
//!
//! This is an early pre-release demo, do not use it in production code!

use std::{
    fmt::Debug,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use imp::FutureLocalKey;
use pin_project::{pin_project, pinned_drop};

mod imp;

/// An init-once-per-future cell for thread-local values.
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

    /// Returns a copy of the contained value.
    ///
    /// # Panics
    ///
    /// This method will panic if the future local doesn't have a value set.
    #[inline]
    pub fn get(&'static self) -> T
    where
        T: Copy,
    {
        self.0.local_key().borrow().unwrap()
    }

    /// Sets a value `T` as the future-local value for the future `F`.
    ///
    /// On completion of `scope`, the future-local value will be dropped.
    #[inline]
    pub fn scope<F>(&'static self, value: T, future: F) -> ScopedFutureWithValue<T, F>
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

/// Attaches future local storage values to a [`Future`].
///
/// Extension trait allowing futures to have their own static variables.
pub trait FutureLocalStorage: Future + Sized + private::Sealed {
    /// Sets a given value as the future local value of this future.
    ///
    /// Each future instance will have its own state of the attached value.
    fn with_scope<T, S>(self, scope: &'static S, value: T) -> ScopedFutureWithValue<T, Self>
    where
        T: Send,
        S: AsRef<FutureLocalKey<T>>;
}

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
        LOCK.0
            .local_key()
            .borrow_mut()
            .replace(RefCell::new("0".to_owned()));

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
        .with_scope(&VALUE, Cell::new(0))
        .discard_value();

        let fut_2 = async { VALUE.with(Cell::get) }
            .with_scope(&VALUE, Cell::new(15))
            .discard_value();

        assert_eq!(fut_1.await, 42);
        assert_eq!(fut_2.await, 15);
        assert_eq!(
            tokio::spawn(
                async { VALUE.with(Cell::get) }
                    .with_scope(&VALUE, Cell::new(115))
                    .discard_value()
            )
            .await
            .unwrap(),
            115
        );
    }
}
