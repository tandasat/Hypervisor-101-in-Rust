//! The module containing the functions to provides exclusive access to UEFI
//! services through the UEFI system table.
//!
//! UEFI services may not be thread-safe, and thus, lock is required.

use spin::{Mutex, MutexGuard};
use uefi::{
    table::{Boot, SystemTable},
    Handle,
};

/// Initializes exclusive access to the system services.
pub(crate) fn init_system_table(system_table: SystemTable<Boot>, image: Handle) {
    unsafe {
        system_table.boot_services().set_image_handle(image);

        assert!(SHARED_SYSTEM_TABLE.is_none());
        SHARED_SYSTEM_TABLE = Some(system_table.unsafe_clone());

        assert!(EXCLUSIVE_SYSTEM_TABLE.is_none());
        let lock = Mutex::new(system_table);
        EXCLUSIVE_SYSTEM_TABLE = Some(lock);
    }
}

/// Returns the UEFI system table  under an exclusive lock.
pub(crate) fn system_table() -> MutexGuard<'static, SystemTable<Boot>> {
    unsafe { EXCLUSIVE_SYSTEM_TABLE.as_ref().unwrap() }.lock()
}

/// Returns the UEFI system table without lock.
///
/// # Safety
/// The UEFI system table must not be used concurrently.
pub(crate) unsafe fn system_table_unsafe() -> SystemTable<Boot> {
    unsafe { SHARED_SYSTEM_TABLE.as_ref().unwrap().unsafe_clone() }
}

static mut SHARED_SYSTEM_TABLE: Option<SystemTable<Boot>> = None;
static mut EXCLUSIVE_SYSTEM_TABLE: Option<Mutex<SystemTable<Boot>>> = None;
