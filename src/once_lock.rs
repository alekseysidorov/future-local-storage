use std::{fmt::Debug, future::Future};

use crate::{
    imp::{self, FutureLocalKey},
    FutureLocalStorage, InstrumentedFuture,
};

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

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use crate::FutureLocalStorage;

    use super::*;

    impl<T: Send + 'static> FutureOnceLock<T> {
        fn replace(&'static self, value: T) -> Option<T> {
            self.0.local_key().borrow_mut().replace(value)
        }

        fn set(&'static self, value: T) {
            self.replace(value);
        }
    }

    #[test]
    fn test_once_lock_trivial() {
        static LOCK: FutureOnceLock<String> = FutureOnceLock::new();
        LOCK.set("0".to_owned());

        assert_eq!(LOCK.with(Clone::clone), "0".to_owned());
        LOCK.set("42".to_owned());
        assert_eq!(LOCK.with(Clone::clone), "42".to_owned());
    }

    #[test]
    fn test_once_lock_multiple_threads() {
        static VALUE: FutureOnceLock<u64> = FutureOnceLock::new();
        VALUE.replace(1);

        let handle = std::thread::spawn(|| {
            assert_eq!(VALUE.get(), None);
            VALUE.replace(2);
            assert_eq!(VALUE.get(), Some(2));
        });

        assert_eq!(VALUE.get(), Some(1));
        handle.join().unwrap();

        assert_eq!(VALUE.get(), Some(1));
    }

    #[tokio::test]
    async fn test_future_once_lock() {
        static VALUE: FutureOnceLock<u64> = FutureOnceLock::new();

        let fut_1 = async {
            for _ in 0..42 {
                let j = VALUE.with(Clone::clone);
                VALUE.replace(j + 1);
                tokio::task::yield_now().await;
            }

            VALUE.get().unwrap()
        }
        .with_scope(&VALUE, 0);

        let fut_2 = async { VALUE.get().unwrap() }.with_scope(&VALUE, 15);

        assert_eq!(fut_1.await, 42);
        assert_eq!(fut_2.await, 15);
        assert_eq!(
            tokio::spawn(async { VALUE.get().unwrap() }.with_scope(&VALUE, 115))
                .await
                .unwrap(),
            115
        );
    }
}
