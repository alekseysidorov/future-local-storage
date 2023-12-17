use std::fmt::Debug;

use crate::imp::{self, FutureLocalKey};

pub struct FutureOnceLock<T>(imp::FutureLocalKey<T>);

impl<T> FutureOnceLock<T> {
    /// Creates an empty future once lock.
    #[must_use]
    pub const fn new() -> Self {
        Self(imp::FutureLocalKey::new())
    }
}

impl<T: Send + 'static> FutureOnceLock<T> {
    #[inline]
    pub fn with<F, R>(&'static self, mut f: F) -> R
    where
        F: FnMut(&Option<T>) -> R,
    {
        let value = self.0.local_key().borrow();
        f(&value)
    }

    #[inline]
    pub fn replace(&'static self, value: T) -> Option<T> {
        self.0.local_key().borrow_mut().replace(value)
    }

    #[inline]
    pub fn swap(&'static self, content: &mut Option<T>) {
        FutureLocalKey::swap(&self.0, content);
    }

    #[inline]
    pub fn get(&'static self) -> Option<T>
    where
        T: Copy,
    {
        *self.0.local_key().borrow()
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

    #[test]
    fn test_once_lock_trivial() {
        static LOCK: FutureOnceLock<String> = FutureOnceLock::new();

        assert_eq!(LOCK.with(Clone::clone), None);
        LOCK.replace("42".to_owned());
        assert_eq!(LOCK.with(Clone::clone), Some("42".to_owned()));
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
}
