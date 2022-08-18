#![allow(dead_code)] // FIXME

use crate::memory_info::{MemorySlot, PythonId};

mod memory_info;
mod metadata;

fn main() {
    let mut data = vec![MemorySlot::empty(); 50];
    let mut infos = memory_info::MemoryPool::from_uninit_slice(&mut data, 200 * 1024 * 1024);

    infos.add_object(PythonId(42), 200).unwrap();

    println!("infos: {infos:#?}");

    // println!("sizeof ArrayInfo: {}", std::mem::size_of::<NdArrayInfo>());
}
