#![allow(dead_code)] // FIXME

use crate::memory_info::PythonId;

mod memory_info;
mod metadata;

fn main() {
    let mut infos = memory_info::MemoryPoolBuilder::default()
        .slot_count(4)
        .memory_size(200 * 1024 * 1024)
        .build();

    infos.add_object(PythonId(42), 200).unwrap();

    println!("infos: {infos:#?}");

    // println!("sizeof ArrayInfo: {}", std::mem::size_of::<NdArrayInfo>());
}
