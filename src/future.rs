//! Future local storage extensions for the [`std::future::Future`].

pub trait FutureLocalStorage<T> {

}

mod private {
    use std::future::Future;

    // use super::{FutureLazy, FutureLock};

    pub trait Sealed {}

    // impl<F: Future> Sealed for F {}
    // impl<T> Sealed for FutureLock<T> {}
    // impl<T> Sealed for FutureLazy<T> {}
}
