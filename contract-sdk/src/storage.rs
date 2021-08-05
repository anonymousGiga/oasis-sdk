//! Smart contract storage interface.
use crate::{
    memory::{HostRegion, HostRegionRef},
    types::storage::StoreKind,
};

#[link(wasm_import_module = "storage")]
extern "wasm" {
    #[link_name = "get"]
    fn storage_get(store: u32, key_ptr: u32, key_len: u32) -> HostRegion;

    #[link_name = "insert"]
    fn storage_insert(store: u32, key_ptr: u32, key_len: u32, value_ptr: u32, value_len: u32);

    #[link_name = "remove"]
    fn storage_remove(store: u32, key_ptr: u32, key_len: u32);
}

/// Fetches a given key from contract storage.
pub fn get(store: StoreKind, key: &[u8]) -> Option<Vec<u8>> {
    let key_region = HostRegionRef::from_slice(key);
    let value_region = unsafe { storage_get(store as u32, key_region.offset, key_region.length) };
    // Special value of (0, 0) is treated as if the key doesn't exist.
    if value_region.offset == 0 && value_region.length == 0 {
        return None;
    }

    Some(value_region.into_vec())
}

/// Inserts a given key/value pair into contract storage.
pub fn insert(store: StoreKind, key: &[u8], value: &[u8]) {
    let key_region = HostRegionRef::from_slice(key);
    let value_region = HostRegionRef::from_slice(value);

    unsafe {
        storage_insert(
            store as u32,
            key_region.offset,
            key_region.length,
            value_region.offset,
            value_region.length,
        );
    }
}

/// Removes a given key from contract storage.
pub fn remove(store: StoreKind, key: &[u8]) {
    let key_region = HostRegionRef::from_slice(key);

    unsafe {
        storage_remove(store as u32, key_region.offset, key_region.length);
    }
}
