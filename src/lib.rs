//! # Overview
#![doc = include_utils::include_md!("README.md:description")]
//!
//! ## Examples
//!
//! ### Tracing spans
//!
//! ```rust
#![doc = include_str!("../examples/context/mod.rs")]
//!
//! // Usage example
//!
//! async fn some_method(mut a: u64) -> u64 {
//!     TracerContext::on_enter(format!("`some_method` with params: a={a}"));
//!
//!     // Some async computation
//!
//!     TracerContext::on_exit("`some_method`");
//!     a * 32
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     let (trace, result) = TracerContext::in_scope(some_method(45)).await;
//!
//!     println!("answer: {result}");
//!     println!("trace: {trace:#?}");
//! }
//! ```

use std::{fmt::Debug, future::Future};

use future::ScopedFutureWithValue;
use imp::FutureLocalKey;

pub mod future;
mod imp;

/// An init-once-per-future cell for thread-local values.
///
/// It uses thread local storage to ensure that the each polled future has its own local storage key.
/// Unlike the [`std::thread::LocalKey`] this cell will *not* lazily initialize the value on first access.
/// Instead, the value is first initialized when the future containing the future-local is first polled
/// by an executor.
///
/// After the execution finished the value moves from the future local cell to the future output.
pub struct FutureOnceCell<T>(imp::FutureLocalKey<T>);

impl<T> FutureOnceCell<T> {
    /// Creates an empty future once cell.
    #[must_use]
    pub const fn new() -> Self {
        Self(imp::FutureLocalKey::new())
    }
}

impl<T> Default for FutureOnceCell<T> {
    #[must_use]
    fn default() -> Self {
        Self::new()
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
    /// - This method will panic if the future local doesn't have a value set.
    ///
    /// - If you the returned future inside the a call to [`Self::with`] on the same cell, then the
    ///   call to `poll` will panic.
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
    /// On completion of `scope`, the future-local value will be returned by the scoped future.
    ///
    /// ```rust
    /// use std::cell::Cell;
    ///
    /// use future_local_storage::FutureOnceCell;
    ///
    /// static VALUE: FutureOnceCell<Cell<u64>> = FutureOnceCell::new();
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let (output, answer) = VALUE.scope(Cell::from(0), async {
    ///         VALUE.with(|x| {
    ///             let value = x.get();
    ///             x.set(value + 1);
    ///         });
    ///
    ///         42
    ///     }).await;
    /// }
    /// ```
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
    ///
    /// ```rust
    /// use std::cell::Cell;
    ///
    /// use future_local_storage::{FutureOnceCell, FutureLocalStorage};
    ///
    /// static VALUE: FutureOnceCell<Cell<u64>> = FutureOnceCell::new();
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let (output, answer) = async {
    ///         VALUE.with(|x| {
    ///             let value = x.get();
    ///             x.set(value + 1);
    ///         });
    ///
    ///         42
    ///     }.with_scope(&VALUE, Cell::from(0)).await;
    /// }
    /// ```
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
    fn test_once_cell_without_future() {
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
    async fn test_future_once_cell_output() {
        static VALUE: FutureOnceCell<Cell<u64>> = FutureOnceCell::new();

        let (output, ()) = VALUE
            .scope(Cell::from(0), async {
                VALUE.with(|x| {
                    let value = x.get();
                    x.set(value + 1);
                });
            })
            .await;

        assert_eq!(output.into_inner(), 1);
    }

    #[tokio::test]
    async fn test_future_once_cell_discard_value() {
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
