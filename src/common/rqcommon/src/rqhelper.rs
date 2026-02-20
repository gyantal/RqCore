use std::sync::{Mutex, MutexGuard, PoisonError};

// ---------- Error helpers: RqError ----------
// std::error::Error is a trait, not a concrete type you can instantiate with new().
// For that you define your own error type (often an enum) and implement the std::error::Error trait for it.
#[derive(Debug)]
pub enum RqError { // In the future, define more different error variants as needed
    Config(String), // Configuration-related errors with a message
    Io(std::io::Error), // Wraps std::io::Error for IO-related issues
}

impl std::error::Error for RqError {} // this is the key: our own error type implements the std::error::Error trait

impl std::fmt::Display for RqError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            RqError::Config(msg) => write!(f, "Configuration error: {}", msg),
            RqError::Io(err) => write!(f, "IO error: {}", err),
        }
    }
}

impl From<std::io::Error> for RqError {
    fn from(err: std::io::Error) -> Self {
        RqError::Io(err)
    }
}

// ---------- Mutex helpers ----------

// Poison event: Occurs when a thread panics while holding the lock. That can leave the data in an inconsistent state, so Rust marks the Mutex as poisoned to signal that it may be unsafe to access.
//     let mut data = c_mutex.lock().unwrap();
//     data.insert(10);
//     panic!(); // this 'might' leave the data structure in an invalid state. Data conditions, relations between data items are not correct.
// The Mutex become poisoned, and subsequent attempts to lock it will return a PoisonError. 
// However, you can choose to ignore the poison and access the data anyway, if you decide that the data is still in a usable state.
// If panics are expected while holding a lock, consider using std::panic::catch_unwind around critical sections to prevent poisoning in the first place.
pub trait MutexExt<T> {
    fn lock_ignore_poison(&self) -> MutexGuard<'_, T>;
}

impl<T> MutexExt<T> for Mutex<T> {
    fn lock_ignore_poison(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(PoisonError::into_inner)
    }
}