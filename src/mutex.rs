use std::cell::RefCell;

use fslock::{LockFile, ToOsStr};

#[derive(Debug)]
pub struct FsMutex {
    lockfile: RefCell<LockFile>,
}

impl FsMutex {
    pub fn open<P>(path: &P) -> Result<Self, fslock::Error>
    where
        P: ToOsStr + ?Sized,
    {
        let lockfile = RefCell::new(LockFile::open(path)?);
        Ok(Self { lockfile })
    }

    pub fn lock(&self) -> Result<LockFileGuard<'_>, fslock::Error> {
        self.lockfile.borrow_mut().lock()?;
        Ok(LockFileGuard {
            lockfile: &self.lockfile,
        })
    }

    pub fn is_locked(&self) -> bool {
        self.lockfile.borrow().owns_lock()
    }
}

#[derive(Debug)]
pub struct LockFileGuard<'a> {
    lockfile: &'a RefCell<LockFile>,
}

impl<'a> Drop for LockFileGuard<'a> {
    fn drop(&mut self) {
        let _ = self.lockfile.borrow_mut().unlock();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fs_mutex() -> anyhow::Result<()> {
        let mutex = FsMutex::open("test_fs_mutex.lock")?;

        assert!(!mutex.is_locked());

        {
            let _guard = mutex.lock()?;
            assert!(mutex.is_locked());
        }

        assert!(!mutex.is_locked());
        Ok(())
    }
}
