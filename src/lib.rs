//! # Overview
//!
//! This is an early pre-release demo, do not use it in production code!

use std::{
    fmt::Debug,
    future::Future,
};

use future::ScopedFutureWithValue;
use imp::FutureLocalKey;


mod imp;
pub mod future;

/// An init-once-per-future cell for thread-local values.
pub struct FutureOnceCell<T>(imp::FutureLocalKey<T>);

impl<T> FutureOnceCell<T> {
    /// Creates an empty future once lock.
    #[must_use]
    pub const fn new() -> Self {
        Self(imp::FutureLocalKey::new())
    }
}

impl<T: Send + 'static> FutureOnceCell<T> {
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

impl<T: Debug + Send + 'static> Debug for FutureOnceCell<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("FutureOnceCell").field(&self.0).finish()
    }
}

impl<T> AsRef<FutureLocalKey<T>> for FutureOnceCell<T> {
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
        static LOCK: FutureOnceCell<RefCell<String>> = FutureOnceCell::new();
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
        static VALUE: FutureOnceCell<Cell<u64>> = FutureOnceCell::new();

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
