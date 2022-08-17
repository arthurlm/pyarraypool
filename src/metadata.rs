use smallstr::SmallString;
use smallvec::SmallVec;

/// Store metadata about NDArray object to be able to reconstruct them
/// given a python object ID and an offset in memory.
#[derive(Debug, PartialEq, Eq, Clone)]
#[repr(C)]
pub struct NdArrayMetadata {
    /// Python object ID.
    python_id: u64,

    /// NDArray data type as string.
    dtype: SmallString<[u8; 10]>,

    /// NDArray shape.
    shape: SmallVec<[u64; 8]>,
}

impl NdArrayMetadata {
    /// Create new item.
    pub fn new(python_id: u64, dtype: &str, shape: &[u64]) -> Self {
        let arr = Self {
            python_id,
            shape: SmallVec::from_slice(shape),
            dtype: SmallString::from_str(dtype),
        };
        assert!(!arr.dtype.spilled(), "dtype corrupted");
        assert!(!arr.shape.spilled(), "shape corrupted");
        arr
    }

    /// Get python object ID.
    pub fn python_id(&self) -> u64 {
        self.python_id
    }

    /// Get data type as string.
    pub fn dtype(&self) -> String {
        self.dtype.to_string()
    }

    /// Get array shape.
    pub fn shape(&self) -> Vec<u64> {
        self.shape.to_vec()
    }

    /// Get used bytes cout given array shape.
    pub fn bytes_count(&self) -> usize {
        if self.shape.is_empty() {
            0
        } else {
            self.shape.iter().product::<u64>() as usize
        }
    }
}

impl Default for NdArrayMetadata {
    fn default() -> Self {
        Self {
            python_id: 0,
            shape: SmallVec::new(),
            dtype: SmallString::from_str("float64"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod ndarray_metadata {
        use super::*;

        #[test]
        fn test_getter_and_setter() {
            let mut x = NdArrayMetadata::default();
            assert_eq!(x.python_id(), 0);
            assert_eq!(x.dtype(), "float64");
            assert_eq!(x.shape(), vec![]);
            assert_eq!(x.bytes_count(), 0);

            x = NdArrayMetadata::new(42, "i64", &[4, 0, 6]);
            assert_eq!(x.python_id(), 42);
            assert_eq!(x.dtype(), "i64");
            assert_eq!(x.shape(), vec![4, 0, 6]);
            assert_eq!(x.bytes_count(), 0);

            x = NdArrayMetadata::new(43, "i32", &[4, 2, 6]);
            assert_eq!(x.python_id(), 43);
            assert_eq!(x.dtype(), "i32");
            assert_eq!(x.shape(), vec![4, 2, 6]);
            assert_eq!(x.bytes_count(), 48);
        }

        #[test]
        #[should_panic]
        fn test_dtype_corruption() {
            NdArrayMetadata::new(42, "some_invalid_data_type", &[]);
        }

        #[test]
        #[should_panic]
        fn test_shape_corruption() {
            NdArrayMetadata::new(42, "float64", &vec![0; 100]);
        }
    }
}
