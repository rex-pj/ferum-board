//! Poison-tolerant `Mutex` access for the plugin runtime.
//!
//! `lock().unwrap()` turns one panic inside plugin dispatch into a permanent
//! outage: the mutex stays poisoned, so every later request through it panics
//! too, until the process restarts. One misbehaving plugin would take the forum
//! down rather than tripping its own circuit breaker.
//!
//! Recovery is sound *for these mutexes specifically*. Each guards data with no
//! cross-field invariant — a dispatch table replaced wholesale, and two
//! `Option<Instant>` timestamps that the next call repairs. Do not reach for
//! this on a lock whose contents can be left half-updated.

use std::sync::{Mutex, MutexGuard};

pub(crate) trait LockUnpoisoned<T> {
    /// `lock()`, recovering the guard if a previous holder panicked.
    fn lock_unpoisoned(&self) -> MutexGuard<'_, T>;
}

impl<T> LockUnpoisoned<T> for Mutex<T> {
    fn lock_unpoisoned(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}
