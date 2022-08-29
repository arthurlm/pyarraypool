#![allow(dead_code)] // FIXME
#![warn(missing_docs)]

/*! python export */

pub mod memory_info;
mod mutex;
pub mod shm;

use std::{os::raw::c_schar, path::PathBuf, str::FromStr, sync::Arc};

use memory_info::PythonId;
use pyo3::{
    exceptions::PyException,
    ffi::{PyBUF_WRITE, PyMemoryView_FromMemory},
};
use pyo3::{ffi::PyMemoryView_Check, prelude::*};
use pyo3::{ffi::Py_ssize_t, types::PyTuple};
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
    pool: Arc<ShmObjectPool<'static>>,
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
            pool: Arc::new(pool),
        })
    }

    fn add_object(&self, python_id: u64, request_size: usize) -> PyResult<PyObject> {
        let data = self.pool.add_object(PythonId(python_id), request_size)?;
        Ok(self.pymemoryview_from_slice(data))
    }

    fn attach_object(&self, python_id: u64) -> PyResult<PyObject> {
        let data = self.pool.attach_object(PythonId(python_id))?;
        Ok(self.pymemoryview_from_slice(data))
    }

    fn detach_object(&self, python_id: u64) -> PyResult<()> {
        self.pool
            .detach_object(PythonId(python_id))
            .map_err(|e| e.into())
    }

    fn set_object_releasable(&self, python_id: u64) -> PyResult<()> {
        self.pool
            .set_object_releasable(PythonId(python_id))
            .map_err(|e| e.into())
    }

    fn memview_of(&self, python_id: u64) -> Option<PyObject> {
        let data = self.pool.slice_of(PythonId(python_id))?;
        Some(self.pymemoryview_from_slice(data))
    }

    fn dump(&self) -> String {
        self.pool.dump()
    }
}

impl PyShmObjectPool {
    fn pymemoryview_from_slice(&self, data: &mut [u8]) -> PyObject {
        Python::with_gil(|py| unsafe {
            let memview_ptr = PyMemoryView_FromMemory(
                data.as_mut_ptr() as *mut c_schar,
                data.len() as Py_ssize_t,
                PyBUF_WRITE,
            );

            assert!(!memview_ptr.is_null());
            assert!(PyMemoryView_Check(memview_ptr) == 1);

            py.from_owned_ptr::<PyAny>(memview_ptr)
                .extract()
                .expect("Fail to create PyObject")
        })
    }
}

#[pymodule]
fn pyarraypool(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_class::<PyShmObjectPool>()?;
    Ok(())
}
