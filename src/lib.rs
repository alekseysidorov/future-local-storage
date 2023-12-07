use std::{
    cell::{Ref, RefCell},
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use pin_project_lite::pin_project;
use state::LocalInitCell;

type LocalKey<T> = RefCell<Option<T>>;

pub struct FutureLock<T>(LocalInitCell<LocalKey<T>>);

impl<T: Send> FutureLock<T> {
    pub const fn new() -> Self {
        Self(LocalInitCell::new())
    }

    fn local_key(&'static self) -> &LocalKey<T> {
        if self.0.try_get().is_none() {
            self.0.set(|| RefCell::new(None));
        }

        self.0.get()
    }

    fn get(&'static self) -> Ref<Option<T>> {
        let local_key = self.local_key();
        let value = local_key.borrow();
        value
    }

    fn attach<F: Future>(&'static self, fut: F) -> InstrumentedFuture<T, F> {
        InstrumentedFuture {
            inner: fut,
            future_lock: self,
            stored_value: None,
        }
    }

    pub fn set(&'static self, value: T) {
        self.local_key().borrow_mut().replace(value);
    }

    pub fn with_or_init<F, R, I>(&'static self, mut with: F, init: I) -> R
    where
        F: FnMut(&T) -> R,
        I: FnOnce() -> T,
    {
        if self.get().is_none() {
            self.set(init());
        }

        with(self.local_key().borrow().as_ref().unwrap())
    }
}

impl<T> FutureLock<T>
where
    T: Copy + Send,
{
    #[must_use]
    pub fn get_or_init<I>(&'static self, init: I) -> T
    where
        I: FnOnce() -> T,
    {
        if self.get().is_none() {
            let value = init();
            self.set(value);
            return value;
        }

        self.local_key().borrow().unwrap()
    }
}

pin_project! {
    pub struct InstrumentedFuture<T: 'static, F> {
        #[pin]
        inner: F,
        future_lock: & 'static FutureLock<T>,
        stored_value: Option<T>,
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
        std::mem::swap(
            this.stored_value,
            &mut *this.future_lock.local_key().borrow_mut(),
        );
        // Poll the underlying future.
        let result = this.inner.poll(cx);
        // Swap future local key back.
        std::mem::swap(
            this.stored_value,
            &mut *this.future_lock.local_key().borrow_mut(),
        );

        result
    }
}

pub struct FutureLazy<T> {
    lock: FutureLock<T>,
    init: fn() -> T,
}

impl<T: Send> FutureLazy<T> {
    pub const fn new(init: fn() -> T) -> Self {
        Self {
            lock: FutureLock::new(),
            init,
        }
    }

    pub fn with<F, R>(&'static self, with: F) -> R
    where
        F: FnMut(&T) -> R,
    {
        self.lock.with_or_init(with, self.init)
    }

    pub fn set(&'static self, value: T) {
        self.lock.set(value)
    }
}

impl<T> FutureLazy<T>
where
    T: Copy + Send,
{
    pub fn get(&'static self) -> T {
        self.with(|x| *x)
    }
}

pub trait Storage<T>: private::Sealed {
    fn attach<F: Future>(&'static self, fut: F) -> InstrumentedFuture<T, F>;
}

impl<T: Send> Storage<T> for FutureLock<T> {
    fn attach<F: Future>(&'static self, fut: F) -> InstrumentedFuture<T, F> {
        InstrumentedFuture {
            inner: fut,
            future_lock: self,
            stored_value: None,
        }
    }
}

impl<T: Send> Storage<T> for FutureLazy<T> {
    fn attach<F: Future>(&'static self, fut: F) -> InstrumentedFuture<T, F> {
        self.lock.attach(fut)
    }
}

pub trait FutureLocalStorage: Sized + private::Sealed {
    fn attach<T: Send, S: Storage<T>>(self, lock: &'static S) -> InstrumentedFuture<T, Self>;
}

impl<F: Future> FutureLocalStorage for F {
    fn attach<T: Send, S: Storage<T>>(self, lock: &'static S) -> InstrumentedFuture<T, Self> {
        lock.attach(self)
    }
}

mod private {
    use std::future::Future;

    use crate::{FutureLazy, FutureLock};

    pub trait Sealed {}

    impl<F: Future> Sealed for F {}
    impl<T> Sealed for FutureLock<T> {}
    impl<T> Sealed for FutureLazy<T> {}
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn test_get_set_trivial() {
        static VALUE: FutureLock<u64> = FutureLock::new();

        assert_eq!(*VALUE.get(), None);
        VALUE.set(42);

        assert_eq!(*VALUE.get(), Some(42));
    }

    #[test]
    fn test_with_init() {
        static VALUE: FutureLock<String> = FutureLock::new();

        let s = VALUE.with_or_init(|s| s.clone(), || "Back to the future".to_owned());
        assert_eq!(s, "Back to the future");
    }

    #[test]
    fn test_set_multiple_threads() {
        static VALUE: FutureLock<u64> = FutureLock::new();
        VALUE.set(1);

        let handle = std::thread::spawn(|| {
            assert_eq!(*VALUE.get(), None);
            VALUE.set(2);
            assert_eq!(*VALUE.get(), Some(2));
        });

        assert_eq!(*VALUE.get(), Some(1));
        handle.join().unwrap();

        assert_eq!(*VALUE.get(), Some(1));
    }

    #[tokio::test]
    async fn test_future_lock_attach() {
        static VALUE: FutureLock<u64> = FutureLock::new();

        let fut_1 = async {
            for _ in 0..42 {
                let j = VALUE.get_or_init(|| 0);
                VALUE.set(j + 1);
                tokio::time::sleep(Duration::from_millis(5)).await;
            }

            VALUE.get().unwrap()
        }
        .attach(&VALUE);

        let fut_2 = async { VALUE.with_or_init(|x| *x, || 15) }.attach(&VALUE);

        assert_eq!(fut_1.await, 42);
        assert_eq!(fut_2.await, 15);
        assert_eq!(
            tokio::spawn(async { VALUE.get_or_init(|| 115) }.attach(&VALUE))
                .await
                .unwrap(),
            115
        );
    }

    #[tokio::test]
    async fn test_future_lazy_attach() {
        static VALUE: FutureLazy<i32> = FutureLazy::new(|| -1);

        let fut_1 = async {
            for _ in 0..42 {
                let j = VALUE.with(|x| *x);
                VALUE.set(j + 1);
                tokio::time::sleep(Duration::from_millis(5)).await;
            }

            VALUE.get()
        }
        .attach(&VALUE);

        let fut_2 = async {
            VALUE.set(15);
            tokio::time::sleep(Duration::from_millis(5)).await;
            VALUE.get()
        }
        .attach(&VALUE);

        assert_eq!(fut_1.await, 41);
        assert_eq!(fut_2.await, 15);
        assert_eq!(
            tokio::spawn(
                async {
                    VALUE.set(115);
                    tokio::time::sleep(Duration::from_millis(5)).await;
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
