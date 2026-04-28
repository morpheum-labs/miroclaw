//! Serialize access to process environment in unit tests (parallel tests share `std::env`).

use std::sync::{Mutex, OnceLock};

pub(crate) fn lock() -> std::sync::MutexGuard<'static, ()> {
    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    ENV_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .expect("test env lock poisoned")
}
