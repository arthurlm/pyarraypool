/*! Helper to manage memory block. */

use std::{fmt, process};

use thiserror::Error;

/// Possible error that can occurs with memory pool management.
#[derive(Debug, PartialEq, Eq, Clone, Copy, Error)]
pub enum ArrayPoolError {
    /// No space left in pool.
    #[error("no space left")]
    NoSpaceLeft,

    /// No free bloc left in pool.
    #[error("no free bloc left")]
    NoFreeBlocLeft,

    /// Object already exists.
    #[error("object already exists (python ID: {0})")]
    ObjectAlreadyExists(PythonId),

    /// Object is not found.
    #[error("object cannot be found: (python ID: {0})")]
    ObjectNotFound(PythonId),

    /// Invalid python object ID.
    #[error("invalid python object ID")]
    InvalidPythonId,
}

/// Wrapper arount u64 to add and restrict python ID values.
#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
#[repr(C)]
pub struct PythonId(pub u64);

impl PythonId {
    const fn valid(&self) -> Result<(), ArrayPoolError> {
        if self.0 == 0 {
            Err(ArrayPoolError::InvalidPythonId)
        } else {
            Ok(())
        }
    }

    const fn empty() -> Self {
        Self(0)
    }
}

impl fmt::Display for PythonId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

const FLAG_MEMSLOT_TRANSFERED: u8 = 0x01;

/// Store information about memory hole.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[repr(C)]
pub struct MemorySlot {
    /// Python object ID.
    python_id: PythonId,

    /// Slot size in bytes.
    size: usize,

    /// Reference object count.
    refcount: usize,

    /// Source process ID
    ///
    /// This will be used to compute flags.
    source_pid: u32,

    /// Associated flags:
    ///
    /// - FLAG_MEMSLOT_TRANSFERED
    flags: u8,
}

impl MemorySlot {
    /// Create empty slot.
    pub const fn empty() -> Self {
        Self {
            python_id: PythonId::empty(),
            size: 0,
            refcount: 0,
            source_pid: 0,
            flags: 0,
        }
    }

    /// Create new slot with size and without data.
    const fn with_size(size: usize) -> Self {
        Self {
            python_id: PythonId::empty(),
            size,
            refcount: 0,
            source_pid: 0,
            flags: 0,
        }
    }

    /// Create new slot with python ID and object size.
    fn with_object_id(python_id: PythonId, size: usize) -> Self {
        Self {
            python_id,
            size,
            refcount: 1,
            source_pid: process::id(),
            flags: 0,
        }
    }

    /// Check if slot is free.
    const fn is_free(&self) -> bool {
        self.python_id.0 == 0
    }

    /// Check if slot can be release.
    ///
    /// IE. Object has been transfered at least once between processes and is unused.
    const fn is_releasable(&self) -> bool {
        (self.flags & FLAG_MEMSLOT_TRANSFERED == FLAG_MEMSLOT_TRANSFERED) && (self.refcount == 0)
    }

    /// Split block to create new free space.
    const fn split_block(&self, bytes_count: usize) -> (Self, Self) {
        assert!(bytes_count < self.size);
        (
            Self {
                python_id: self.python_id,
                size: bytes_count,
                refcount: self.refcount,
                source_pid: self.source_pid,
                flags: self.flags,
            },
            Self::with_size(self.size - bytes_count),
        )
    }

    /// Set reference count.
    fn set_refcount(&mut self, refcount: usize) -> Self {
        self.refcount = refcount;
        *self
    }

    /// Mark memory slot as transfered between processes.
    fn set_transfered(&mut self) -> Self {
        self.flags |= FLAG_MEMSLOT_TRANSFERED;
        *self
    }

    /// Update internal flags based.
    fn update_flags(&mut self) {
        if self.source_pid != process::id() {
            self.set_transfered();
        }
    }
}

/// Contains information about existing object in memory pool.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct ObjectInfo {
    offset: usize,
    size: usize,
}

impl ObjectInfo {
    /// Create new object.
    pub fn new(offset: usize, size: usize) -> Self {
        Self { offset, size }
    }

    /// Get object offset.
    pub fn offset(&self) -> usize {
        self.offset
    }

    /// Get object size.
    pub fn size(&self) -> usize {
        self.size
    }
}

/// Vector of memory slot with associated function to manage them.
#[derive(Debug, PartialEq, Eq)]
#[repr(C)]
pub struct MemoryPool<'a> {
    slots: &'a mut [MemorySlot],
}

impl<'a> MemoryPool<'a> {
    /// Create struct from already init vec.
    pub fn new(slots: &'a mut [MemorySlot]) -> Self {
        Self { slots }
    }

    /// Create new structure from uninitialized slice.
    pub fn from_uninit_slice(slots: &'a mut [MemorySlot], data_size: usize) -> Self {
        assert!(!slots.is_empty());
        assert!(data_size > 0);

        // Init array
        for slot in slots.iter_mut().skip(1) {
            *slot = MemorySlot::empty();
        }
        slots[0] = MemorySlot::with_size(data_size);

        Self::new(slots)
    }

    /// Add new object to pool.
    pub fn add_object(
        &mut self,
        python_id: PythonId,
        request_size: usize,
    ) -> Result<usize, ArrayPoolError> {
        python_id.valid()?;

        // Check object does not already exists
        if self.slots.iter().any(|x| x.python_id == python_id) {
            return Err(ArrayPoolError::ObjectAlreadyExists(python_id));
        }

        // Find free space
        let target_idx = self
            .slots
            .iter()
            .position(|slot| slot.is_free() && request_size <= slot.size)
            .ok_or(ArrayPoolError::NoSpaceLeft)?;

        if request_size < self.slots[target_idx].size {
            let slot_len = self.slots.len();
            debug_assert!(slot_len > 0);

            // Check we still have free block
            if self.slots[slot_len - 1] != MemorySlot::empty() {
                return Err(ArrayPoolError::NoFreeBlocLeft);
            }

            // Move everything to allow insert of new free object
            self.slots[target_idx + 1..slot_len].rotate_right(1);
            debug_assert!(self.slots[target_idx + 1].is_free());

            // Fill info in newly created space
            (self.slots[target_idx], self.slots[target_idx + 1]) =
                self.slots[target_idx].split_block(request_size);
        }

        // Fill memory info
        self.slots[target_idx] = MemorySlot::with_object_id(python_id, request_size);

        // Get address of newly created bloc
        Ok(self.offset_by_index(target_idx))
    }

    /// Increase ref count usage by 1 for a given python object.
    pub fn attach_object(&mut self, python_id: PythonId) -> Result<ObjectInfo, ArrayPoolError> {
        python_id.valid()?;

        // Get object index
        let object_index = self
            .slots
            .iter()
            .position(|slot| slot.python_id == python_id)
            .ok_or(ArrayPoolError::ObjectNotFound(python_id))?;

        // Increase refcount and update internals
        self.slots[object_index].refcount += 1;
        self.slots[object_index].update_flags();

        Ok(ObjectInfo::new(
            self.offset_by_index(object_index),
            self.slots[object_index].size,
        ))
    }

    /// Decrease ref count usage by 1 for a given python object.
    ///
    /// If reference count reach 0, object will be remove from pool.
    pub fn detach_object(&mut self, python_id: PythonId) -> Result<(), ArrayPoolError> {
        python_id.valid()?;

        let object_index = self
            .slots
            .iter()
            .position(|slot| slot.python_id == python_id)
            .ok_or(ArrayPoolError::ObjectNotFound(python_id))?;

        // Decrease reference count and release slot if now unused.
        if self.slots[object_index].refcount > 0 {
            self.slots[object_index].refcount -= 1;
        }

        if self.slots[object_index].is_releasable() {
            self.release_offset(object_index);
        }

        Ok(())
    }

    /// Set python ID object as now releasable. Even if it has not been transfered
    /// between processes.
    pub fn set_object_releasable(&mut self, python_id: PythonId) -> Result<(), ArrayPoolError> {
        python_id.valid()?;
        let object_index = self
            .slots
            .iter()
            .position(|slot| slot.python_id == python_id)
            .ok_or(ArrayPoolError::ObjectNotFound(python_id))?;

        // Update flags and release slot if now unused.
        self.slots[object_index].set_transfered();
        if self.slots[object_index].is_releasable() {
            self.release_offset(object_index);
        }

        Ok(())
    }

    /// Get offset for a given object index.
    fn offset_by_index(&self, object_index: usize) -> usize {
        self.slots[..object_index].iter().map(|x| x.size).sum()
    }

    /// Update internal slots to mark slot as now free.
    fn release_offset(&mut self, object_index: usize) {
        debug_assert!(!self.slots[object_index].is_free());
        let slot_len = self.slots.len();

        // Mark bloc as now free
        self.slots[object_index] = MemorySlot::with_size(self.slots[object_index].size);

        // Merge with next bloc if free
        if object_index + 1 < slot_len && self.slots[object_index + 1].is_free() {
            self.slots[object_index].size += self.slots[object_index + 1].size;
            self.slots[object_index + 1] = MemorySlot::empty();
            self.slots[object_index + 1..slot_len].rotate_left(1);
        }

        // Merge with previous block if free
        if object_index > 0 && self.slots[object_index - 1].is_free() {
            self.slots[object_index - 1].size += self.slots[object_index].size;
            self.slots[object_index] = MemorySlot::empty();
            self.slots[object_index..slot_len].rotate_left(1);
        }
    }

    /// Get object info of given python object.
    pub fn info_of(&self, python_id: PythonId) -> Option<ObjectInfo> {
        python_id.valid().ok()?;

        let position = self.slots.iter().position(|x| x.python_id == python_id)?;
        Some(ObjectInfo::new(
            self.offset_by_index(position),
            self.slots[position].size,
        ))
    }

    /// Dump memory content as a string.
    pub fn dump(&self) -> String {
        self.slots
            .iter()
            .enumerate()
            .filter(|(_id, slot)| !slot.is_free())
            .map(|(id, slot)| {
                format!(
                    "SLOT ID: {id}: pid: {0}, recount: {1}, flag: {2}",
                    slot.python_id, slot.refcount, slot.flags
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod memory_slot {
        use super::*;

        #[test]
        fn test_attributes() {
            let mut slot = MemorySlot::empty();
            assert!(slot.is_free());

            slot.python_id = PythonId(42);
            assert!(!slot.is_free());
        }

        #[test]
        fn test_releasable() {
            let mut slot = MemorySlot::with_object_id(PythonId(12), 150);
            assert!(!slot.is_releasable());

            // Detach from main process
            slot.refcount = 0;
            assert!(!slot.is_releasable());

            // Attach and detach again from main process
            slot.refcount = 1;
            assert!(!slot.is_releasable());
            slot.refcount = 0;
            assert!(!slot.is_releasable());

            // Attach from sub process
            slot.refcount = 1;
            slot.set_transfered();
            assert!(!slot.is_releasable());

            // Release from sub process => not releasable
            slot.refcount = 0;
            assert!(slot.is_releasable());
        }

        #[test]
        fn test_split_block() {
            let python_id = PythonId(42);
            let refcount = 4;

            let slot = MemorySlot::with_object_id(python_id, 200)
                .set_refcount(refcount)
                .set_transfered();

            assert_eq!(
                slot.split_block(50),
                (
                    MemorySlot {
                        python_id,
                        size: 50,
                        refcount,
                        source_pid: std::process::id(),
                        flags: FLAG_MEMSLOT_TRANSFERED,
                    },
                    MemorySlot {
                        python_id: PythonId::empty(),
                        size: 150,
                        refcount: 0,
                        source_pid: 0,
                        flags: 0,
                    },
                )
            );
            assert_eq!(
                slot.split_block(0),
                (
                    MemorySlot {
                        python_id,
                        size: 0,
                        refcount,
                        source_pid: std::process::id(),
                        flags: FLAG_MEMSLOT_TRANSFERED,
                    },
                    MemorySlot {
                        python_id: PythonId::empty(),
                        size: 200,
                        refcount: 0,
                        source_pid: 0,
                        flags: 0,
                    },
                )
            );
            assert_eq!(
                slot.split_block(199),
                (
                    MemorySlot {
                        python_id,
                        size: 199,
                        refcount,
                        source_pid: std::process::id(),
                        flags: FLAG_MEMSLOT_TRANSFERED,
                    },
                    MemorySlot {
                        python_id: PythonId::empty(),
                        size: 1,
                        refcount: 0,
                        source_pid: 0,
                        flags: 0,
                    },
                )
            );
        }

        #[test]
        #[should_panic]
        fn test_split_block_fail() {
            let slot = MemorySlot {
                python_id: PythonId(42),
                size: 200,
                refcount: 3,
                source_pid: std::process::id(),
                flags: 0,
            };

            let (_, _) = slot.split_block(200);
        }
    }

    mod memory_pool {
        use super::*;

        const MEMORY_SIZE: usize = 10 * 1024;
        const SLOT_COUNT: usize = 4;

        #[test]
        fn test_init() {
            let mut slots = vec![MemorySlot::empty(); SLOT_COUNT];
            let memory = MemoryPool::from_uninit_slice(&mut slots, MEMORY_SIZE);
            assert_eq!(
                memory.slots,
                vec![
                    MemorySlot::with_size(MEMORY_SIZE),
                    MemorySlot::empty(),
                    MemorySlot::empty(),
                    MemorySlot::empty(),
                ]
            );
        }

        #[test]
        fn test_add_invalid_python_id() {
            let mut slots = vec![MemorySlot::empty(); SLOT_COUNT];
            let mut memory = MemoryPool::from_uninit_slice(&mut slots, MEMORY_SIZE);
            assert_eq!(
                memory.add_object(PythonId::empty(), 10),
                Err(ArrayPoolError::InvalidPythonId)
            );
        }

        #[test]
        fn test_add_duplicated_object() {
            let mut slots = vec![MemorySlot::empty(); SLOT_COUNT];
            let mut memory = MemoryPool::from_uninit_slice(&mut slots, MEMORY_SIZE);

            assert_eq!(memory.add_object(PythonId(40), 10), Ok(0));
            assert_eq!(
                memory.add_object(PythonId(40), 10),
                Err(ArrayPoolError::ObjectAlreadyExists(PythonId(40)))
            );
        }

        #[test]
        fn test_add_huge_bloc() {
            let mut slots = vec![MemorySlot::empty(); SLOT_COUNT];
            let mut memory = MemoryPool::from_uninit_slice(&mut slots, MEMORY_SIZE);

            assert_eq!(
                memory.add_object(PythonId(40), MEMORY_SIZE + 1),
                Err(ArrayPoolError::NoSpaceLeft)
            );
        }

        #[test]
        fn test_add_bloc() {
            let mut slots = vec![MemorySlot::empty(); SLOT_COUNT];
            let mut memory = MemoryPool::from_uninit_slice(&mut slots, MEMORY_SIZE);

            assert_eq!(memory.add_object(PythonId(40), 150), Ok(0));
            assert_eq!(memory.add_object(PythonId(41), 50), Ok(150));
            assert_eq!(memory.add_object(PythonId(42), 5 * 1024), Ok(200));

            assert_eq!(
                memory.slots,
                vec![
                    MemorySlot::with_object_id(PythonId(40), 150),
                    MemorySlot::with_object_id(PythonId(41), 50),
                    MemorySlot::with_object_id(PythonId(42), 5 * 1024),
                    MemorySlot::with_size(MEMORY_SIZE - 150 - 50 - (5 * 1024)),
                ]
            );
        }

        #[test]
        fn test_add_single_huge_bloc() {
            let mut slots = vec![MemorySlot::empty(); SLOT_COUNT];
            let mut memory = MemoryPool::from_uninit_slice(&mut slots, MEMORY_SIZE);

            // Add bloc
            assert_eq!(memory.add_object(PythonId(42), MEMORY_SIZE), Ok(0));

            // Check we do not have space anymore
            assert_eq!(
                memory.add_object(PythonId(43), 1),
                Err(ArrayPoolError::NoSpaceLeft)
            );

            // Check inner content
            assert_eq!(
                memory.slots,
                vec![
                    MemorySlot::with_object_id(PythonId(42), MEMORY_SIZE),
                    MemorySlot::empty(),
                    MemorySlot::empty(),
                    MemorySlot::empty(),
                ]
            );
        }

        #[test]
        fn test_add_empty_bloc() {
            let mut slots = vec![MemorySlot::empty(); SLOT_COUNT];
            let mut memory = MemoryPool::from_uninit_slice(&mut slots, MEMORY_SIZE);

            // Add blocs
            assert_eq!(memory.add_object(PythonId(40), 0), Ok(0));
            assert_eq!(memory.add_object(PythonId(41), 0), Ok(0));
            assert_eq!(memory.add_object(PythonId(42), 0), Ok(0));
            assert_eq!(
                memory.slots,
                vec![
                    MemorySlot::with_object_id(PythonId(40), 0),
                    MemorySlot::with_object_id(PythonId(41), 0),
                    MemorySlot::with_object_id(PythonId(42), 0),
                    MemorySlot::with_size(MEMORY_SIZE),
                ]
            );

            // Add final bloc
            assert_eq!(
                memory.add_object(PythonId(43), 0),
                Err(ArrayPoolError::NoFreeBlocLeft)
            );

            // Add bloc with data
            assert_eq!(memory.add_object(PythonId(44), MEMORY_SIZE), Ok(0));
            assert_eq!(
                memory.slots,
                vec![
                    MemorySlot::with_object_id(PythonId(40), 0),
                    MemorySlot::with_object_id(PythonId(41), 0),
                    MemorySlot::with_object_id(PythonId(42), 0),
                    MemorySlot::with_object_id(PythonId(44), MEMORY_SIZE),
                ]
            );

            // Check we cannot add anything more
            assert_eq!(
                memory.add_object(PythonId(43), 0),
                Err(ArrayPoolError::NoSpaceLeft)
            );
        }

        #[test]
        fn test_detach_invalid_python_id() {
            let mut slots = vec![MemorySlot::empty(); SLOT_COUNT];
            let mut memory = MemoryPool::from_uninit_slice(&mut slots, MEMORY_SIZE);
            assert_eq!(
                memory.detach_object(PythonId::empty()),
                Err(ArrayPoolError::InvalidPythonId)
            );
        }

        #[test]
        fn test_detach_not_found() {
            let mut slots = vec![MemorySlot::empty(); SLOT_COUNT];
            let mut memory = MemoryPool::from_uninit_slice(&mut slots, MEMORY_SIZE);

            assert_eq!(
                memory.detach_object(PythonId(42)),
                Err(ArrayPoolError::ObjectNotFound(PythonId(42)))
            );
        }

        #[test]
        fn test_add_and_detach() {
            let mut slots = vec![MemorySlot::empty(); SLOT_COUNT];
            let mut memory = MemoryPool::from_uninit_slice(&mut slots, MEMORY_SIZE);

            // Add
            assert!(memory.add_object(PythonId(40), 10).is_ok());
            assert_eq!(
                memory.slots,
                vec![
                    MemorySlot::with_object_id(PythonId(40), 10),
                    MemorySlot::with_size(MEMORY_SIZE - 10),
                    MemorySlot::empty(),
                    MemorySlot::empty(),
                ]
            );

            // Detach
            assert_eq!(memory.detach_object(PythonId(40)), Ok(()));
            assert_eq!(memory.set_object_releasable(PythonId(40)), Ok(()));
            assert_eq!(
                memory.slots,
                vec![
                    MemorySlot::with_size(MEMORY_SIZE),
                    MemorySlot::empty(),
                    MemorySlot::empty(),
                    MemorySlot::empty(),
                ]
            );
        }

        #[test]
        fn test_detach_elements() {
            let mut slots = vec![MemorySlot::empty(); SLOT_COUNT];
            let mut memory = MemoryPool::from_uninit_slice(&mut slots, MEMORY_SIZE);

            // Add
            assert!(memory.add_object(PythonId(40), 10).is_ok());
            assert!(memory.add_object(PythonId(41), 10).is_ok());
            assert!(memory.add_object(PythonId(42), 10).is_ok());

            // Detach middle one
            assert_eq!(memory.detach_object(PythonId(41)), Ok(()));
            assert_eq!(memory.set_object_releasable(PythonId(41)), Ok(()));
            assert_eq!(memory.info_of(PythonId(40)), Some(ObjectInfo::new(0, 10)));
            assert_eq!(memory.info_of(PythonId(42)), Some(ObjectInfo::new(20, 10)));
            assert_eq!(
                memory.slots,
                vec![
                    MemorySlot::with_object_id(PythonId(40), 10),
                    MemorySlot::with_size(10),
                    MemorySlot::with_object_id(PythonId(42), 10),
                    MemorySlot::with_size(MEMORY_SIZE - 10 - 10 - 10),
                ]
            );

            // Detach previous
            assert_eq!(memory.set_object_releasable(PythonId(40)), Ok(()));
            assert_eq!(memory.detach_object(PythonId(40)), Ok(()));
            assert_eq!(memory.info_of(PythonId(42)), Some(ObjectInfo::new(20, 10)));
            assert_eq!(
                memory.slots,
                vec![
                    MemorySlot::with_size(20),
                    MemorySlot::with_object_id(PythonId(42), 10),
                    MemorySlot::with_size(MEMORY_SIZE - 10 - 10 - 10),
                    MemorySlot::empty(),
                ]
            );

            // Detach next
            assert_eq!(memory.set_object_releasable(PythonId(42)), Ok(()));
            assert_eq!(memory.detach_object(PythonId(42)), Ok(()));
            assert_eq!(
                memory.slots,
                vec![
                    MemorySlot::with_size(MEMORY_SIZE),
                    MemorySlot::empty(),
                    MemorySlot::empty(),
                    MemorySlot::empty(),
                ]
            );
        }

        #[test]
        fn test_fill_holes() {
            let mut slots = vec![MemorySlot::empty(); SLOT_COUNT];
            let mut memory = MemoryPool::from_uninit_slice(&mut slots, MEMORY_SIZE);

            // Add
            assert!(memory.add_object(PythonId(40), 10).is_ok());
            assert!(memory.add_object(PythonId(41), 10).is_ok());
            assert!(memory.add_object(PythonId(42), 10).is_ok());

            // Detach 40 and 41
            assert_eq!(memory.detach_object(PythonId(40)), Ok(()));
            assert_eq!(memory.detach_object(PythonId(41)), Ok(()));
            assert_eq!(memory.set_object_releasable(PythonId(40)), Ok(()));
            assert_eq!(memory.set_object_releasable(PythonId(41)), Ok(()));
            assert_eq!(
                memory.slots,
                vec![
                    MemorySlot::with_size(20),
                    MemorySlot::with_object_id(PythonId(42), 10),
                    MemorySlot::with_size(MEMORY_SIZE - 10 - 10 - 10),
                    MemorySlot::empty(),
                ]
            );

            // Insert new smaller array
            assert!(memory.add_object(PythonId(43), 15).is_ok());
            assert_eq!(
                memory.slots,
                vec![
                    MemorySlot::with_object_id(PythonId(43), 15),
                    MemorySlot::with_size(5),
                    MemorySlot::with_object_id(PythonId(42), 10),
                    MemorySlot::with_size(MEMORY_SIZE - 10 - 10 - 10),
                ]
            );

            assert!(memory.add_object(PythonId(44), 5).is_ok());
            assert_eq!(
                memory.slots,
                vec![
                    MemorySlot::with_object_id(PythonId(43), 15),
                    MemorySlot::with_object_id(PythonId(44), 5),
                    MemorySlot::with_object_id(PythonId(42), 10),
                    MemorySlot::with_size(MEMORY_SIZE - 10 - 10 - 10),
                ]
            );
        }

        #[test]
        fn test_info_of_invalid_python_id() {
            let mut slots = vec![MemorySlot::empty(); SLOT_COUNT];
            let memory = MemoryPool::from_uninit_slice(&mut slots, MEMORY_SIZE);
            assert_eq!(memory.info_of(PythonId::empty()), None);
        }

        #[test]
        fn test_info_of_missing_obj() {
            let mut slots = vec![MemorySlot::empty(); SLOT_COUNT];
            let memory = MemoryPool::from_uninit_slice(&mut slots, MEMORY_SIZE);
            assert_eq!(memory.info_of(PythonId(42)), None);
        }

        #[test]
        fn test_info_of() {
            let mut slots = vec![MemorySlot::empty(); SLOT_COUNT];
            let mut memory = MemoryPool::from_uninit_slice(&mut slots, MEMORY_SIZE);
            let python_id = PythonId(42);

            assert_eq!(memory.info_of(python_id), None);

            assert!(memory.add_object(python_id, 10).is_ok());
            assert_eq!(memory.info_of(python_id), Some(ObjectInfo::new(0, 10)));

            assert!(memory.detach_object(python_id).is_ok());
            assert_eq!(memory.info_of(python_id), Some(ObjectInfo::new(0, 10)));

            assert!(memory.set_object_releasable(python_id).is_ok());
            assert_eq!(memory.info_of(python_id), None);
        }

        #[test]
        fn test_attach_object_invalid_python_id() {
            let mut slots = vec![MemorySlot::empty(); SLOT_COUNT];
            let mut memory = MemoryPool::from_uninit_slice(&mut slots, MEMORY_SIZE);
            assert_eq!(
                memory.attach_object(PythonId::empty()),
                Err(ArrayPoolError::InvalidPythonId)
            );
        }

        #[test]
        fn test_attach_object_missing() {
            let mut slots = vec![MemorySlot::empty(); SLOT_COUNT];
            let mut memory = MemoryPool::from_uninit_slice(&mut slots, MEMORY_SIZE);
            assert_eq!(
                memory.attach_object(PythonId(40)),
                Err(ArrayPoolError::ObjectNotFound(PythonId(40)))
            );
        }

        #[test]
        fn test_attach_object() {
            let python_id1 = PythonId(40);
            let python_id2 = PythonId(41);
            let mut slots = vec![MemorySlot::empty(); SLOT_COUNT];
            let mut memory = MemoryPool::from_uninit_slice(&mut slots, MEMORY_SIZE);

            assert!(memory.add_object(python_id1, 20).is_ok());
            assert!(memory.add_object(python_id2, 10).is_ok());

            // Attach multiple time object
            assert_eq!(memory.attach_object(python_id1), Ok(ObjectInfo::new(0, 20)));
            assert_eq!(
                memory.attach_object(python_id2),
                Ok(ObjectInfo::new(20, 10))
            );

            assert_eq!(memory.attach_object(python_id1), Ok(ObjectInfo::new(0, 20)));
            assert_eq!(
                memory.attach_object(python_id2),
                Ok(ObjectInfo::new(20, 10))
            );

            assert_eq!(memory.attach_object(python_id1), Ok(ObjectInfo::new(0, 20)));

            // Check memory
            assert_eq!(
                memory.slots,
                vec![
                    MemorySlot::with_object_id(python_id1, 20).set_refcount(4),
                    MemorySlot::with_object_id(python_id2, 10).set_refcount(3),
                    MemorySlot::with_size(MEMORY_SIZE - 20 - 10),
                    MemorySlot::empty(),
                ]
            );

            // Start detaching
            assert_eq!(memory.set_object_releasable(python_id1), Ok(()));
            assert_eq!(memory.detach_object(python_id1), Ok(()));
            assert_eq!(memory.detach_object(python_id1), Ok(()));
            assert_eq!(memory.detach_object(python_id1), Ok(()));
            assert_eq!(memory.detach_object(python_id1), Ok(()));
            assert_eq!(
                memory.detach_object(python_id1),
                Err(ArrayPoolError::ObjectNotFound(python_id1))
            );

            assert_eq!(memory.set_object_releasable(python_id2), Ok(()));
            assert_eq!(memory.detach_object(python_id2), Ok(()));
            assert_eq!(memory.detach_object(python_id2), Ok(()));
            assert_eq!(memory.detach_object(python_id2), Ok(()));
            assert_eq!(
                memory.detach_object(python_id2),
                Err(ArrayPoolError::ObjectNotFound(python_id2))
            );
        }

        #[test]
        fn test_dump() -> anyhow::Result<()> {
            let mut slots = vec![MemorySlot::empty(); SLOT_COUNT];
            let mut memory = MemoryPool::from_uninit_slice(&mut slots, MEMORY_SIZE);

            // Add
            memory.add_object(PythonId(40), 10)?;
            memory.add_object(PythonId(41), 10)?;
            memory.set_object_releasable(PythonId(41))?;
            memory.add_object(PythonId(42), 10)?;

            // Dump
            assert_eq!(
                memory.dump(),
                "SLOT ID: 0: pid: 40, recount: 1, flag: 0\n\
                 SLOT ID: 1: pid: 41, recount: 1, flag: 1\n\
                 SLOT ID: 2: pid: 42, recount: 1, flag: 0"
            );

            Ok(())
        }
    }
}
