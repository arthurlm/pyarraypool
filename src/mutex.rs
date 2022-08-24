use std::sync::atomic::{AtomicBool, Ordering};

/// Simple spin lock based on atomic u8
///
/// See: https://doc.rust-lang.org/nomicon/atomics.html
#[derive(Debug)]
pub struct SimpleSpinLock {
    lock: AtomicBool,
}

impl SimpleSpinLock {
    /// Init spin lock
    pub const fn new() -> Self {
        Self {
            lock: AtomicBool::new(false),
        }
    }

    pub fn lock(&self) -> SimpleSpinLockGuard<'_> {
        while self
            .lock
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Acquire)
            .is_err()
        {}

        SimpleSpinLockGuard { lock: &self.lock }
    }

    pub fn is_locked(&self) -> bool {
        self.lock.load(Ordering::Relaxed)
    }
}

pub struct SimpleSpinLockGuard<'a> {
    lock: &'a AtomicBool,
}

impl<'a> Drop for SimpleSpinLockGuard<'a> {
    fn drop(&mut self) {
        self.lock.store(false, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mutex() {
        let mutex = SimpleSpinLock::new();

        assert!(!mutex.is_locked());

        {
            let _guard = mutex.lock();
            assert!(mutex.is_locked());
        }

        assert!(!mutex.is_locked());
    }
}
