pub use future::{FutureLocalStorage, InstrumentedFuture};

mod future;
mod imp;

/// A value which is initialized on the first access.
///
/// Similar to [`std::cell::LazyCell`], but local to a certain future.
pub struct FutureLazyLock<T> {
    inner: imp::FutureLocalKey<T>,
    // TODO Rewrite on top of unsafe cell.
    init: fn() -> T,
}

impl<T: Send> FutureLazyLock<T> {
    /// Creates an empty future lazy lock.
    #[inline]
    pub const fn new(init: fn() -> T) -> Self {
        Self {
            inner: imp::FutureLocalKey::new(),
            init,
        }
    }

    /// Returns a reference to a local key, initalizing it with the `init` if it has not been
    /// previously initialized.
    #[inline]
    fn inited_local_key(&'static self) -> &'static imp::LocalKey<T> {
        let is_inited = self.inner.local_key().borrow().is_some();
        // Local key is empty only before init, so this branch runs only once.
        if !is_inited {
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
        self.inner.local_key().borrow_mut().replace(value);
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

pub struct FutureOnceLock<T>(imp::FutureLocalKey<T>);

impl<T: Send> FutureOnceLock<T> {
    /// Creates an empty future once lock.
    pub const fn new() -> Self {
        Self(imp::FutureLocalKey::new())
    }

    #[inline]
    pub fn with<F, R>(&'static self, f: F) -> R
    where
        F: FnOnce(&Option<T>) -> R,
    {
        let value = self.0.local_key().borrow();
        f(&value)
    }

    #[inline]
    pub fn replace(&'static self, value: T) -> Option<T> {
        self.0.local_key().borrow_mut().replace(value)
    }

    #[inline]
    pub fn get(&'static self) -> Option<T>
    where
        T: Copy,
    {
        *self.0.local_key().borrow()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use pretty_assertions::assert_eq;

    #[test]
    fn test_once_lock_trivial() {
        static LOCK: FutureOnceLock<String> = FutureOnceLock::new();

        assert_eq!(LOCK.with(|x| x.clone()), None);
        LOCK.replace("42".to_owned());
        assert_eq!(LOCK.with(|x| x.clone()), Some("42".to_owned()));
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

    #[test]
    fn test_lazy_lock_trivial() {
        static LOCK: FutureLazyLock<&str> = FutureLazyLock::new(|| "42");

        assert_eq!(LOCK.with(|x| *x), "42");
        // LOCK.replace("abacaba");
        // assert_eq!(LOCK.get(), "abacaba");
    }

    #[test]
    fn test_lazy_lock_multiple_threads() {
        static VALUE: FutureLazyLock<u64> = FutureLazyLock::new(|| 1);

        let handle = std::thread::spawn(|| {
            assert_eq!(VALUE.get(), 1);
            VALUE.replace(2);
            assert_eq!(VALUE.get(), 2);
        });

        assert_eq!(VALUE.get(), 1);
        handle.join().unwrap();
        // Make sure that after the thread will be finished, the value will not change.
        assert_eq!(VALUE.get(), 1);
    }
}
