//! Low-level implementation details.

use std::{cell::RefCell, fmt::Debug};

use state::LocalInitCell;

/// A type wrapper that provides interior mutability and allows for safe and efficient access to an
/// optional value stored in a cell.
pub type LocalKey<T> = RefCell<Option<T>>;

/// A future local storage key which owns its content.
///
/// It uses thread local storage to ensure that the each polled future has its own local storage key.
pub struct FutureLocalKey<T>(LocalInitCell<LocalKey<T>>);

impl<T> FutureLocalKey<T> {
    /// Creates an empty future local key.
    #[inline]
    pub const fn new() -> Self {
        Self(LocalInitCell::new())
    }
}

impl<T: Send + 'static> FutureLocalKey<T> {
    /// Returns a reference to the underlying thread local storage key, and if it has not been initalized,
    /// initializes it with the `None` value.
    ///
    /// # Important
    ///
    /// Using this method ensures that the local key is initialized, use only it ot access the underlying
    /// thread local key.
    #[inline]
    pub fn local_key(&'static self) -> &'static LocalKey<T> {
        self.0.set(|| RefCell::new(None));
        self.0.get()
    }

    /// Swaps the underlying value and the given one, without deinitializing either one.
    #[inline]
    pub fn swap(this: &'static Self, other: &mut Option<T>) {
        std::mem::swap(other, &mut *this.local_key().borrow_mut());
    }
}

impl<T: Send + 'static + Debug> Debug for FutureLocalKey<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("FutureLocalKey").field(&self.0).finish()
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, thread::JoinHandle};

    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_future_local_key_init() {
        static KEY: FutureLocalKey<i32> = FutureLocalKey::new();

        assert_eq!(*KEY.local_key().borrow(), None);
    }

    #[test]
    fn test_future_local_key_swap() {
        static KEY: FutureLocalKey<String> = FutureLocalKey::new();

        let threads = (0..42).map(|i| {
            std::thread::spawn(move || {
                let mut slot = Some(i.to_string());
                // Swap keys and make sure that the slot and key content actually has been swapped.
                FutureLocalKey::swap(&KEY, &mut slot);
                assert_eq!(slot, None);
                assert_eq!(*KEY.local_key().borrow(), Some(i.to_string()));

                // Swap keys again.
                FutureLocalKey::swap(&KEY, &mut slot);
                assert_eq!(slot, Some(i.to_string()));
                assert_eq!(*KEY.local_key().borrow(), None);
            })
        });

        threads.into_iter().try_for_each(JoinHandle::join).unwrap();
    }

    // Test [`state::LocalInitCell`] itself.
    #[test]
    fn test_local_init_cell_multiple_threads() {
        static VALUE: LocalInitCell<Cell<usize>> = LocalInitCell::new();
        VALUE.set(|| Cell::new(0));

        let handle = std::thread::spawn(|| {
            assert_eq!(VALUE.get().get(), 0);
            VALUE.get().set(2);
            assert_eq!(VALUE.get().get(), 2);
        });

        assert_eq!(VALUE.get().get(), 0);
        handle.join().unwrap();
        // Make sure that after the thread will be finished, the value will not change.
        assert_eq!(VALUE.get().get(), 0);
    }
}
