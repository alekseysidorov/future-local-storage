# Overview

This crate provides an [`FutureOnceCell`] cell-like type, which provides the
similar API as the [`tokio::task_local`] but without using any macros.

Future local storage associates a value to the context of a given future. After
the future finished it returns this value back to the caller. That meaning that
the values is passed through the context of the executed future. This
functionality can be useful for tracing async code or adding metrics to it.

## Usage

```rust
use std::cell::Cell;

use future_local_storage::FutureOnceCell;

static VALUE: FutureOnceCell<Cell<u64>> = FutureOnceCell::new();

#[tokio::main]
async fn main() {
    let (output, answer) = VALUE.scope(Cell::from(0), async {
        VALUE.with(|x| {
            let value = x.get();
            x.set(value + 1);
        });

        "42".to_owned()
    }).await;

    assert_eq!(output.into_inner(), 1);
    assert_eq!(answer, "42");
}
```
