use std::{cell::{Ref, RefMut}, ops::Deref};

mod imp;
mod future;
pub mod old;

pub struct FutureOnceLock<T>(imp::FutureLocalKey<T>);

impl<T: Send> FutureOnceLock<T> {
    /// Creates an empty future once lock.
    pub const fn new() -> Self {
        Self(imp::FutureLocalKey::new())
    }

    #[inline]
    pub fn borrow(&'static self) -> Ref<Option<T>> {
        self.0.local_key().borrow()
    }

    #[inline]
    pub fn borrow_mut(&'static self) -> RefMut<Option<T>> {
        self.0.local_key().borrow_mut()
    }
}

impl<T: Send + Copy> FutureOnceLock<T> {
    #[inline]
    pub fn get(&'static self) -> Option<T> {
        *self.borrow()
    }

    #[inline]
    pub fn set(&'static self, value: T) {
        self.borrow_mut().replace(value);
    }
}

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

    #[inline]
    fn inited_local_key(&'static self) -> &'static imp::LocalKey<T> {
        let is_inited = self.inner.local_key().borrow().is_none();
        // Local key is empty only before init, so this branch runs only once.
        if !is_inited {
            let mut value = Some((self.init)());
            imp::FutureLocalKey::swap(&self.inner, &mut value);
        }
        self.inner.local_key()
    }

    #[inline]
    pub fn borrow(&'static self) -> Ref<T> {
        Ref::map(self.inited_local_key().borrow(), |x| x.as_ref().unwrap())
    }

    #[inline]
    pub fn borrow_mut(&'static self) -> RefMut<T> {
        RefMut::map(self.inited_local_key().borrow_mut(), |x| {
            x.as_mut().unwrap()
        })
    }
}

impl<T: Send + Copy> FutureLazyLock<T> {
    #[inline]
    pub fn get(&'static self) -> T {
        *self.borrow()
    }

    #[inline]
    pub fn set(&'static self, value: T) {
        *self.borrow_mut() = value;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use pretty_assertions::assert_eq;

    #[test]
    fn test_once_lock_trivial() {
        static LOCK: FutureOnceLock<&str> = FutureOnceLock::new();

        assert_eq!(*LOCK.borrow(), None);
        LOCK.borrow_mut().replace("42");
        assert_eq!(*LOCK.borrow(), Some("42"));
    }

    #[test]
    fn test_once_lock_multiple_threads() {
        static VALUE: FutureOnceLock<u64> = FutureOnceLock::new();
        VALUE.set(1);

        let handle = std::thread::spawn(|| {
            assert_eq!(VALUE.get(), None);
            VALUE.set(2);
            assert_eq!(VALUE.get(), Some(2));
        });

        assert_eq!(VALUE.get(), Some(1));
        handle.join().unwrap();

        assert_eq!(VALUE.get(), Some(1));
    }

    #[test]
    fn test_lazy_lock_trivial() {
        static LOCK: FutureLazyLock<&str> = FutureLazyLock::new(|| "42");

        assert_eq!(*LOCK.borrow(), "42");
        *LOCK.borrow_mut() = "abacaba";
        assert_eq!(*LOCK.borrow(), "abacaba");
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
}
