//! The module containing the [`BootTimeAllocator`] type.

use crate::{size_to_pages, system_table::system_table};
use core::alloc::{GlobalAlloc, Layout};
use uefi::table::boot::{AllocateType, MemoryType};

/// The global allocator based on the UEFI boot services. Any memory allocated
/// by this cannot be used after the `ExitBootServices` UEFI runtime service is
/// called. This project never lets a boot loader call that service, so not an
/// issue.
struct BootTimeAllocator;

#[allow(clippy::cast_ptr_alignment)]
unsafe impl GlobalAlloc for BootTimeAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        let align = layout.align();

        // If the requested alignment is a multiple of 4KB, use `allocate_pages`
        // which allocates 4KB aligned memory with 4KB granularity.
        if (align % 0x1000) == 0 {
            system_table()
                .boot_services()
                .allocate_pages(
                    AllocateType::AnyPages,
                    MemoryType::BOOT_SERVICES_DATA,
                    size_to_pages(size),
                )
                .unwrap_or(0) as *mut u8
        } else if align > 8 {
            // Allocate more space for alignment.
            let Ok(ptr) = system_table()
                .boot_services()
                .allocate_pool(MemoryType::BOOT_SERVICES_DATA, size + align)
            else {
                return core::ptr::null_mut();
            };
            // Calculate align offset.
            let ptr = ptr.as_ptr();
            let mut offset = ptr.align_offset(align);
            if offset == 0 {
                offset = align;
            }
            let return_ptr = unsafe { ptr.add(offset) };
            // Store allocated pointer before the struct.
            unsafe { return_ptr.cast::<*mut u8>().sub(1).write(ptr) };
            return_ptr
        } else {
            system_table()
                .boot_services()
                .allocate_pool(MemoryType::BOOT_SERVICES_DATA, size)
                .map_or(core::ptr::null_mut(), core::ptr::NonNull::as_ptr)
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if (layout.align() % 0x1000) == 0 {
            unsafe {
                system_table()
                    .boot_services()
                    .free_pages(ptr as u64, size_to_pages(layout.size()))
                    .unwrap();
            };
        } else if layout.align() > 8 {
            let ptr = unsafe { ptr.cast::<*mut u8>().sub(1).read() };
            unsafe { system_table().boot_services().free_pool(ptr).unwrap() };
        } else {
            unsafe { system_table().boot_services().free_pool(ptr).unwrap() };
        }
    }
}

#[global_allocator]
static ALLOCATOR: BootTimeAllocator = BootTimeAllocator;
