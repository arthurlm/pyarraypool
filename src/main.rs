#![allow(dead_code)] // FIXME
#![warn(missing_docs)]

/*! main section */

use crate::{memory_info::PythonId, shm::ShmObjectPoolBuilder};

pub mod memory_info;
mod mutex;
pub mod shm;

fn main() {
    let shm = ShmObjectPoolBuilder::new()
        .slot_count(4)
        .data_size(20 * 1024)
        .create()
        .unwrap();

    shm.add_object(PythonId(10), 150).unwrap();
    shm.add_object(PythonId(8), 10 * 1024).unwrap();

    println!("infos: {shm:#?}");
    std::thread::sleep(std::time::Duration::from_secs(30));
}
