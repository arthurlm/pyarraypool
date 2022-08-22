#![allow(dead_code)] // FIXME
#![warn(missing_docs)]

/*! python export */

pub mod memory_info;
mod mutex;
pub mod shm;

use std::{
    path::PathBuf,
    str::FromStr,
    sync::{Arc, Mutex},
};

use memory_info::PythonId;
use pyo3::types::PyTuple;
use pyo3::{exceptions::PyException, types::PyByteArray};
use pyo3::{ffi::PyMemoryView_FromMemory, prelude::*};
use shm::{ShmError, ShmObjectPool};

use crate::shm::ShmObjectPoolBuilder;

impl From<ShmError> for PyErr {
    fn from(err: ShmError) -> Self {
        PyException::new_err(err.to_string())
    }
}

#[pyclass(
    name = "ShmObjectPool",
    text_signature = "(*, slot_count = ..., data_size = ..., path = ...)"
)]
struct PyShmObjectPool {
    pool: Arc<Mutex<ShmObjectPool<'static>>>,
}

unsafe impl Send for PyShmObjectPool {}

#[pymethods]
impl PyShmObjectPool {
    #[new]
    #[args(
        _py_args = "*",
        slot_count = "5000",
        data_size = "524288000",
        path = "\"pyarraypool.seg\""
    )]
    fn new(_py_args: &PyTuple, slot_count: usize, data_size: usize, path: &str) -> PyResult<Self> {
        let path = PathBuf::from_str(path)?;

        let pool = if path.exists() {
            ShmObjectPool::open(path)?
        } else {
            ShmObjectPoolBuilder::new()
                .slot_count(slot_count)
                .data_size(data_size)
                .segment_path(path)
                .create()?
        };

        Ok(Self {
            pool: Arc::new(Mutex::new(pool)),
        })
    }

    fn add_object(&self, python_id: u64, request_size: usize) -> PyResult<usize> {
        self.pool
            .lock()
            .expect("Pool poisoned")
            .add_object(PythonId(python_id), request_size)
            .map_err(|e| e.into())
    }

    fn attach_object(&self, python_id: u64) -> PyResult<usize> {
        self.pool
            .lock()
            .expect("Pool poisoned")
            .attach_object(PythonId(python_id))
            .map_err(|e| e.into())
    }

    fn detach_object(&self, python_id: u64) -> PyResult<()> {
        self.pool
            .lock()
            .expect("Pool poisoned")
            .detach_object(PythonId(python_id))
            .map_err(|e| e.into())
    }

    fn offset_of(&self, python_id: u64) -> Option<usize> {
        self.pool
            .lock()
            .expect("Pool poisoned")
            .offset_of(PythonId(python_id))
    }
}

#[pymodule]
fn pyarraypool(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_class::<PyShmObjectPool>()?;
    Ok(())
}
