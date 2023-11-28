use std::{cell::RefCell, sync::OnceLock};

use state::LocalInitCell;

type LocalKey<T> = RefCell<Option<T>>;

pub struct FutureLock<T: 'static>(LocalInitCell<LocalKey<T>>);

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

    pub fn get(&'static self) -> Option<&T> {
        let local_key = self.local_key();
        let value = local_key.borrow();
        todo!()
    }
}
