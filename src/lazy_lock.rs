use std::fmt::Debug;

use crate::imp::{self, FutureLocalKey};

/// A value which is initialized on the first access.
///
/// Similar to [`std::cell::LazyCell`], but local to a certain future.
pub struct FutureLazyLock<T> {
    inner: imp::FutureLocalKey<T>,
    // TODO Rewrite on top of unsafe cell.
    init: fn() -> T,
}

impl<T> FutureLazyLock<T> {
    /// Creates an empty future lazy lock.
    #[inline]
    pub const fn new(init: fn() -> T) -> Self {
        Self {
            inner: imp::FutureLocalKey::new(),
            init,
        }
    }
}

impl<T: Send + 'static> FutureLazyLock<T> {
    /// Returns a reference to a local key, initalizing it with the `init` if it has not been
    /// previously initialized.
    #[inline]
    fn inited_local_key(&'static self) -> &'static imp::LocalKey<T> {
        // Local key is empty only before init, so this branch runs only once.
        if !self.inner.local_key().borrow().is_some() {
            let mut value = Some((self.init)());
            imp::FutureLocalKey::swap(&self.inner, &mut value);
        }
        self.inner.local_key()
    }

    /// Acquires a reference to the value stored in this future local storage.
    ///
    /// This will lazy initialize value if the future has not referenced this key yet.
    #[inline]
    pub fn with<F, R>(&'static self, f: F) -> R
    where
        F: FnOnce(&T) -> R,
    {
        let value = self.inited_local_key().borrow();
        f(value.as_ref().unwrap())
    }

    /// Replaces a value stored in this future local storage by the given one and returns the
    /// previously stored value.
    ///
    /// This will lazy initialize value if the future has not referenced this key yet.
    #[inline]
    pub fn replace(&'static self, value: T) -> T {
        self.inited_local_key().borrow_mut().replace(value).unwrap()
    }

    /// Sets or initializes the contained value.
    ///
    /// Unlike the other methods, this will not run the lazy initializer of this storage.
    #[inline]
    pub fn set(&'static self, value: T) {
        self.inited_local_key().borrow_mut().replace(value);
    }

    /// Returns a copy of the contained value.
    ///
    /// This will lazy initialize value if the future has not referenced this key yet.
    #[inline]
    pub fn get(&'static self) -> T
    where
        T: Copy,
    {
        self.with(|x| *x)
    }
}

impl<T: Debug + Send + 'static> Debug for FutureLazyLock<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("FutureLazyLock").field(&self.inner).finish()
    }
}

impl<T> AsRef<FutureLocalKey<T>> for FutureLazyLock<T> {
    fn as_ref(&self) -> &FutureLocalKey<T> {
        &self.inner
    }
}

#[cfg(test)]
mod tests {
    use crate::FutureLocalStorage;

    use super::*;

    use pretty_assertions::assert_eq;

    #[test]
    fn test_lazy_lock_trivial() {
        static LOCK: FutureLazyLock<&str> = FutureLazyLock::new(|| "42");

        assert_eq!(LOCK.with(|x| *x), "42");
        LOCK.replace("abacaba");
        assert_eq!(LOCK.get(), "abacaba");
    }

    #[test]
    fn test_lazy_lock_multiple_threads() {
        static VALUE: FutureLazyLock<u64> = FutureLazyLock::new(|| 1);

        let handle = std::thread::spawn(|| {
            assert_eq!(VALUE.get(), 1);
            VALUE.set(2);
            assert_eq!(VALUE.get(), 2);
        });

        assert_eq!(VALUE.get(), 1);
        handle.join().unwrap();
        // Make sure that after the thread will be finished, the value will not change.
        assert_eq!(VALUE.get(), 1);
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
