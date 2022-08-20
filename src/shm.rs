/*! Helper around SHM management. */

use std::{
    cell::RefCell,
    fmt,
    marker::PhantomData,
    path::{Path, PathBuf},
};

use shared_memory::{Shmem, ShmemConf, ShmemError};
use thiserror::Error;

use crate::{
    memory_info::{ArrayPoolError, MemoryPool, MemorySlot, PythonId},
    mutex::FsMutex,
};

const SHM_HEADER_MAGIC: u64 = 0xFF45_9831_ABAB_0001;
const SHM_VERSION: u8 = 1;

const SHM_HEADER_SIZE: usize = std::mem::size_of::<ShmHeader>();
const MEMORY_SLOT_SIZE: usize = std::mem::size_of::<MemorySlot>();

/// Possible error that can occurs with shm module.
#[derive(Debug, PartialEq, Eq, Clone, Error)]
pub enum ShmError {
    /// Header does not contains valid SHM value.
    #[error("invalid shm magic value")]
    InvalidShmMagicValue,

    /// Header does not contains valid SHM version number.
    #[error("invalid shm version")]
    InvalidShmVersion,

    /// Cannot attach SHM file or lock file.
    #[error("library failure: {0}")]
    FileSystemError(String),

    /// Error occurs in memory pool management.
    #[error("error with memory pool: {0}")]
    PoolError(#[from] ArrayPoolError),
}

impl From<ShmemError> for ShmError {
    fn from(err: ShmemError) -> Self {
        Self::FileSystemError(err.to_string())
    }
}

impl From<fslock::Error> for ShmError {
    fn from(err: fslock::Error) -> Self {
        Self::FileSystemError(err.to_string())
    }
}

/// Header of shm.
///
/// Helps to check if module is correctly map to a valid shm segment.
#[derive(Debug)]
#[repr(C)]
pub struct ShmHeader {
    magic: u64,
    version: u8,
    slot_count: usize,
}

impl ShmHeader {
    /// Create new header.
    pub const fn new(slot_count: usize) -> Self {
        Self {
            magic: SHM_HEADER_MAGIC,
            version: SHM_VERSION,
            slot_count,
        }
    }

    /// Check header contains valid data.
    pub const fn valid(&self) -> Result<(), ShmError> {
        if self.magic != SHM_HEADER_MAGIC {
            Err(ShmError::InvalidShmMagicValue)
        } else if self.version != SHM_VERSION {
            Err(ShmError::InvalidShmVersion)
        } else {
            Ok(())
        }
    }
}

/// Shm bind memory object pool.
pub struct ShmObjectPool<'a> {
    shmem: Shmem,
    fs_mutex: FsMutex,
    memory_pool: RefCell<MemoryPool<'a>>,
    offset_data: usize,
    _marker: PhantomData<&'a Shmem>,
}

impl<'a> ShmObjectPool<'a> {
    /// Create struct reading existing shm.
    pub fn open<P>(segment_path: P) -> Result<Self, ShmError>
    where
        P: AsRef<Path>,
    {
        // Open SHM and lockfile
        let segment_path = segment_path.as_ref();
        let shmem = ShmemConf::new().flink(segment_path).open()?;
        let fs_mutex = FsMutex::open(&segment_path.with_extension("lock"))?;

        let mut raw_ptr = shmem.as_ptr();

        // Read and check header
        let header = unsafe { &*(raw_ptr as *const ShmHeader) };
        header.valid()?;

        // Read slots array
        let slots = unsafe {
            raw_ptr = raw_ptr.add(SHM_HEADER_SIZE);
            std::slice::from_raw_parts_mut(raw_ptr as *mut MemorySlot, header.slot_count)
        };

        // Create struct
        Ok(ShmObjectPool {
            shmem,
            fs_mutex,
            memory_pool: RefCell::new(MemoryPool::new(slots)),
            offset_data: SHM_HEADER_SIZE + header.slot_count * MEMORY_SLOT_SIZE,
            _marker: PhantomData,
        })
    }

    /// Add object to shm.
    pub fn add_object(&self, python_id: PythonId, request_size: usize) -> Result<usize, ShmError> {
        let _guard = self.fs_mutex.lock()?;
        let offset = self
            .memory_pool
            .borrow_mut()
            .add_object(python_id, request_size)?;

        Ok(offset + self.offset_data)
    }

    /// Mark object as used by current process.
    pub fn attach_object(&self, python_id: PythonId) -> Result<usize, ShmError> {
        let _guard = self.fs_mutex.lock()?;
        let offset = self.memory_pool.borrow_mut().attach_object(python_id)?;
        Ok(offset + self.offset_data)
    }

    /// Un-mark object as used by current process.
    pub fn detach_object(&self, python_id: PythonId) -> Result<(), ShmError> {
        let _guard = self.fs_mutex.lock()?;
        self.memory_pool.borrow_mut().detach_object(python_id)?;
        Ok(())
    }

    /// Get memory offset of given object.
    pub fn offset_of(&self, python_id: PythonId) -> Option<usize> {
        let _guard = self.fs_mutex.lock().ok()?;
        let offset = self.memory_pool.borrow().offset_of(python_id)?;
        Some(offset + self.offset_data)
    }
}

impl<'a> fmt::Debug for ShmObjectPool<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ShmObjectPool").finish()
    }
}

/// Helper to create new shm object pool.
#[derive(Debug)]
pub struct ShmObjectPoolBuilder {
    slot_count: usize,
    data_size: usize,
    segment_path: PathBuf,
}

impl ShmObjectPoolBuilder {
    /// Create default pool configuration.
    pub fn new() -> Self {
        Self {
            slot_count: 10_000,
            data_size: 512 * 1024 * 1024,
            segment_path: "/dev/shm/obj_pool.seg".into(),
        }
    }

    /// Set pool slot count.
    pub fn slot_count(mut self, value: usize) -> Self {
        self.slot_count = value;
        self
    }

    /// Set pool data size.
    pub fn data_size(mut self, value: usize) -> Self {
        self.data_size = value;
        self
    }

    /// Set pool file path.
    pub fn segment_path<P>(mut self, value: P) -> Self
    where
        P: AsRef<Path>,
    {
        self.segment_path = value.as_ref().into();
        self
    }

    /// Create pool with current configuration.
    pub fn create<'a>(&self) -> Result<ShmObjectPool<'a>, ShmError> {
        let size = SHM_HEADER_SIZE + self.slot_count * MEMORY_SLOT_SIZE + self.data_size;

        // Open segment
        let shmem = ShmemConf::new()
            .size(size)
            .flink(&self.segment_path)
            .create()?;
        let fs_mutex = FsMutex::open(&self.segment_path.with_extension("lock"))?;

        let mut raw_ptr = shmem.as_ptr();

        // Init header
        let header = unsafe { &mut *(raw_ptr as *mut ShmHeader) };
        *header = ShmHeader::new(self.slot_count);

        // Create object pool
        let slots = unsafe {
            raw_ptr = raw_ptr.add(SHM_HEADER_SIZE);
            std::slice::from_raw_parts_mut(raw_ptr as *mut MemorySlot, header.slot_count)
        };

        Ok(ShmObjectPool {
            shmem,
            fs_mutex,
            memory_pool: RefCell::new(MemoryPool::from_uninit_slice(slots, self.data_size)),
            offset_data: SHM_HEADER_SIZE + header.slot_count * MEMORY_SLOT_SIZE,
            _marker: PhantomData,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod shm_header {
        use super::*;

        #[test]
        fn test_valid() {
            let mut header = ShmHeader::new(10);
            assert_eq!(header.valid(), Ok(()));

            header.version = SHM_VERSION + 1;
            assert_eq!(header.valid(), Err(ShmError::InvalidShmVersion));

            header.magic = 0x00;
            assert_eq!(header.valid(), Err(ShmError::InvalidShmMagicValue));
        }
    }

    mod shm_object_pool {
        use super::*;

        #[test]
        fn test_create_and_open() -> anyhow::Result<()> {
            let segment_path = "test_create_and_open.seg";
            let python_id = PythonId(20);

            let pool1 = ShmObjectPoolBuilder::new()
                .segment_path(segment_path)
                .create()?;
            let pool2 = ShmObjectPool::open(segment_path)?;

            // Check pool is empty
            assert_eq!(pool1.offset_of(python_id), None);
            assert_eq!(pool2.offset_of(python_id), None);

            // Add object
            pool1.add_object(python_id, 100)?;

            // Check it is here in both pool
            assert!(pool1.offset_of(python_id).is_some());
            let offset = pool1.offset_of(python_id).unwrap();

            assert_eq!(pool1.offset_of(python_id), Some(offset));
            assert_eq!(pool2.offset_of(python_id), Some(offset));

            // Attach object from pool2, and detach from pool1
            assert_eq!(pool2.attach_object(python_id), Ok(offset));
            assert_eq!(pool1.detach_object(python_id), Ok(()));

            assert_eq!(pool1.offset_of(python_id), Some(offset));
            assert_eq!(pool2.offset_of(python_id), Some(offset));

            // Detach from pool2
            assert_eq!(pool2.detach_object(python_id), Ok(()));

            assert_eq!(pool1.offset_of(python_id), None);
            assert_eq!(pool2.offset_of(python_id), None);

            Ok(())
        }
    }
}