use thiserror::Error;

#[derive(Debug, PartialEq, Eq, Error)]
pub enum ArrayPoolError {
    #[error("no space left")]
    NoSpaceLeft,

    #[error("no free bloc left")]
    NoFreeBlocLeft,

    #[error("object already exists")]
    ObjectAlreadyExists,

    #[error("object cannot be found")]
    ObjectNotFound,

    #[error("invalid python object ID")]
    InvalidPythonId,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
pub struct PythonId(pub u64);

impl PythonId {
    fn valid(&self) -> Result<(), ArrayPoolError> {
        if self.0 == 0 {
            Err(ArrayPoolError::InvalidPythonId)
        } else {
            Ok(())
        }
    }
}

/// Store information about memory hole.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[repr(C)]
struct MemorySlot {
    /// Python object ID.
    python_id: PythonId,

    /// Slot size in bytes.
    size: usize,
}

impl MemorySlot {
    /// Create empty slot.
    const fn empty() -> Self {
        Self {
            python_id: PythonId(0),
            size: 0,
        }
    }

    /// Create new slot with size and without data.
    const fn with_size(size: usize) -> Self {
        Self {
            python_id: PythonId(0),
            size,
        }
    }

    /// Create new slot with python ID and object size.
    const fn with_object_id(python_id: PythonId, size: usize) -> Self {
        Self { python_id, size }
    }

    /// Check if slot is free.
    const fn is_free(&self) -> bool {
        self.python_id.0 == 0
    }

    /// Split block to create new free space.
    const fn split_block(&self, bytes_count: usize) -> (Self, Self) {
        assert!(bytes_count < self.size);
        (
            Self::with_object_id(self.python_id, bytes_count),
            Self::with_size(self.size - bytes_count),
        )
    }
}

/// Vector of memory slot with associated function to manage them.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct MemoryPool {
    slots: Vec<MemorySlot>,
}

impl MemoryPool {
    /// Add new object to pool.
    pub fn add_object(
        &mut self,
        python_id: PythonId,
        request_size: usize,
    ) -> Result<usize, ArrayPoolError> {
        python_id.valid()?;

        // Check object does not already exists
        if self.slots.iter().any(|x| x.python_id == python_id) {
            return Err(ArrayPoolError::ObjectAlreadyExists);
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
        Ok(self.slots[..target_idx].iter().map(|x| x.size).sum())
    }

    /// Remove object from pool.
    pub fn remove_object(&mut self, python_id: PythonId) -> Result<(), ArrayPoolError> {
        python_id.valid()?;

        let object_index = self
            .slots
            .iter()
            .position(|slot| slot.python_id == python_id)
            .ok_or(ArrayPoolError::ObjectNotFound)?;

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

        Ok(())
    }

    /// Get offset of given python object.
    pub fn offset_of(&self, python_id: PythonId) -> Option<usize> {
        python_id.valid().ok()?;

        let position = self.slots.iter().position(|x| x.python_id == python_id)?;
        let addr = self.slots[..position].iter().map(|x| x.size).sum();
        Some(addr)
    }
}

/// Builder for memory pool.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct MemoryPoolBuilder {
    slot_count: usize,
    memory_size: usize,
}

impl MemoryPoolBuilder {
    /// Set slot count.
    pub const fn slot_count(mut self, value: usize) -> Self {
        assert!(value > 0);
        self.slot_count = value;
        self
    }

    /// Set memory size.
    pub const fn memory_size(mut self, value: usize) -> Self {
        assert!(value > 0);
        self.memory_size = value;
        self
    }

    /// Build new pool from current config.
    pub fn build(&self) -> MemoryPool {
        debug_assert!(self.memory_size > 0);
        debug_assert!(self.slot_count > 0);

        let mut x = MemoryPool {
            slots: vec![MemorySlot::empty(); self.slot_count],
        };
        x.slots[0].size = self.memory_size;

        x
    }
}

impl Default for MemoryPoolBuilder {
    fn default() -> Self {
        Self {
            slot_count: 1000,
            memory_size: 500 * 1024 * 1024,
        }
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
        fn test_split_block() {
            let python_id = PythonId(42);
            let slot = MemorySlot::with_object_id(python_id, 200);

            assert_eq!(
                slot.split_block(50),
                (
                    MemorySlot {
                        python_id,
                        size: 50,
                    },
                    MemorySlot {
                        python_id: Default::default(),
                        size: 150,
                    },
                )
            );
            assert_eq!(
                slot.split_block(0),
                (
                    MemorySlot { python_id, size: 0 },
                    MemorySlot {
                        python_id: Default::default(),
                        size: 200,
                    },
                )
            );
            assert_eq!(
                slot.split_block(199),
                (
                    MemorySlot {
                        python_id,
                        size: 199,
                    },
                    MemorySlot {
                        python_id: Default::default(),
                        size: 1,
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
            };

            let (_, _) = slot.split_block(200);
        }
    }

    mod memory_pool {
        use super::*;

        const MEMORY_SIZE: usize = 10 * 1024;

        macro_rules! memory_pool {
            () => {
                MemoryPoolBuilder::default()
                    .slot_count(4)
                    .memory_size(MEMORY_SIZE)
                    .build()
            };
        }

        #[test]
        fn test_init() {
            let memory = memory_pool!();
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
            let mut memory = memory_pool!();
            assert_eq!(
                memory.add_object(PythonId(0), 10),
                Err(ArrayPoolError::InvalidPythonId)
            );
        }

        #[test]
        fn test_add_duplicated_object() {
            let mut memory = memory_pool!();

            assert_eq!(memory.add_object(PythonId(40), 10), Ok(0));
            assert_eq!(
                memory.add_object(PythonId(40), 10),
                Err(ArrayPoolError::ObjectAlreadyExists)
            );
        }

        #[test]
        fn test_add_huge_bloc() {
            let mut memory = memory_pool!();

            assert_eq!(
                memory.add_object(PythonId(40), MEMORY_SIZE + 1),
                Err(ArrayPoolError::NoSpaceLeft)
            );
        }

        #[test]
        fn test_add_bloc() {
            let mut memory = memory_pool!();

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
            let mut memory = memory_pool!();

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
            let mut memory = memory_pool!();

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
        fn test_remove_invalid_python_id() {
            let mut memory = memory_pool!();
            assert_eq!(
                memory.remove_object(PythonId(0)),
                Err(ArrayPoolError::InvalidPythonId)
            );
        }

        #[test]
        fn test_remove_not_found() {
            let mut memory = memory_pool!();

            assert_eq!(
                memory.remove_object(PythonId(42)),
                Err(ArrayPoolError::ObjectNotFound)
            );
        }

        #[test]
        fn test_add_and_remove() {
            let mut memory = memory_pool!();

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

            // Remove
            assert_eq!(memory.remove_object(PythonId(40)), Ok(()));
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
        fn test_remove_elements() {
            let mut memory = memory_pool!();

            // Add
            assert!(memory.add_object(PythonId(40), 10).is_ok());
            assert!(memory.add_object(PythonId(41), 10).is_ok());
            assert!(memory.add_object(PythonId(42), 10).is_ok());

            // Remove middle one
            assert_eq!(memory.remove_object(PythonId(41)), Ok(()));
            assert_eq!(memory.offset_of(PythonId(40)), Some(0));
            assert_eq!(memory.offset_of(PythonId(42)), Some(20));
            assert_eq!(
                memory.slots,
                vec![
                    MemorySlot::with_object_id(PythonId(40), 10),
                    MemorySlot::with_size(10),
                    MemorySlot::with_object_id(PythonId(42), 10),
                    MemorySlot::with_size(MEMORY_SIZE - 10 - 10 - 10),
                ]
            );

            // Remove previous
            assert_eq!(memory.remove_object(PythonId(40)), Ok(()));
            assert_eq!(memory.offset_of(PythonId(42)), Some(20));
            assert_eq!(
                memory.slots,
                vec![
                    MemorySlot::with_size(20),
                    MemorySlot::with_object_id(PythonId(42), 10),
                    MemorySlot::with_size(MEMORY_SIZE - 10 - 10 - 10),
                    MemorySlot::empty(),
                ]
            );

            // Remove next
            assert_eq!(memory.remove_object(PythonId(42)), Ok(()));
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
            let mut memory = memory_pool!();

            // Add
            assert!(memory.add_object(PythonId(40), 10).is_ok());
            assert!(memory.add_object(PythonId(41), 10).is_ok());
            assert!(memory.add_object(PythonId(42), 10).is_ok());

            // Remove 40 and 41
            assert_eq!(memory.remove_object(PythonId(40)), Ok(()));
            assert_eq!(memory.remove_object(PythonId(41)), Ok(()));
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
        fn test_offset_of_invalid_python_id() {
            let memory = memory_pool!();
            assert_eq!(memory.offset_of(PythonId(0)), None);
        }

        #[test]
        fn test_offset_of_missing_obj() {
            let memory = memory_pool!();
            assert_eq!(memory.offset_of(PythonId(42)), None);
        }

        #[test]
        fn test_offset_of() {
            let mut memory = memory_pool!();
            let python_id = PythonId(42);

            assert_eq!(memory.offset_of(python_id), None);

            assert!(memory.add_object(python_id, 10).is_ok());
            assert_eq!(memory.offset_of(python_id), Some(0));

            assert!(memory.remove_object(python_id).is_ok());
            assert_eq!(memory.offset_of(python_id), None);
        }
    }
}
