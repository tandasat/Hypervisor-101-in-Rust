//! The module containing types and functions to read the snapshot file.

use crate::{
    disk::{get_file_info, open_file, read_page_from_snapshot},
    global_state::GlobalState,
    size_to_pages, Page,
};
use alloc::{boxed::Box, vec::Vec};
use bit_vec::BitVec;
use core::ptr::addr_of;
use log::{debug, error, info};
use uefi::proto::media::file::{Directory, RegularFile};
use x86::current::paging::{BASE_PAGE_SHIFT, BASE_PAGE_SIZE};

/// The current state and contents of the snapshot.
///
/// The basic idea is that it will allocate large enough memory to be able to
/// store the entire contents of the snapshot file (copy of physical memory
/// taken as a snapshot). Then, read the contents of the snapshot file only on
/// demand, that is, when nested page fault occurs. See also README.md.
pub(crate) struct Snapshot {
    pub(crate) memory: Box<[Page]>,
    pub(crate) registers: SnapshotRegisters,
    memory_ranges: Vec<SnapshotMemoryRange>,
    read_bitmap: BitVec,
    resolved_page_count: u64,
    snapshot_file: RegularFile,
}

/// The collection of register values captured in the snapshot file.
#[derive(derivative::Derivative)]
#[derivative(Debug)]
#[repr(C)]
pub(crate) struct SnapshotRegisters {
    pub(crate) gdtr: x86::dtables::DescriptorTablePointer<u64>, // +0x0
    #[derivative(Debug = "ignore")]
    pub(crate) _padding1:
        [u8; 0x10 - core::mem::size_of::<x86::dtables::DescriptorTablePointer<u64>>()],
    pub(crate) idtr: x86::dtables::DescriptorTablePointer<u64>, // +0x10
    #[derivative(Debug = "ignore")]
    pub(crate) _padding2:
        [u8; 0x10 - core::mem::size_of::<x86::dtables::DescriptorTablePointer<u64>>()],
    pub(crate) es: u16, // +0x20
    pub(crate) cs: u16,
    pub(crate) ss: u16,
    pub(crate) ds: u16,
    pub(crate) fs: u16,
    pub(crate) gs: u16,
    pub(crate) ldtr: u16,
    pub(crate) tr: u16,
    pub(crate) efer: u64, // +0x30
    pub(crate) sysenter_cs: u64,
    pub(crate) cr0: u64, // +0x40
    pub(crate) cr3: u64,
    pub(crate) cr4: u64, // +0x50
    pub(crate) fs_base: u64,
    pub(crate) gs_base: u64, // +0x60
    pub(crate) ldtr_base: u64,
    pub(crate) tr_base: u64, // +0x70
    pub(crate) rsp: u64,
    pub(crate) rip: u64, // +0x80
    pub(crate) rflags: u64,
    pub(crate) sysenter_esp: u64, // +0x90
    pub(crate) sysenter_eip: u64,
    pub(crate) rax: u64, // +0xa0
    pub(crate) rbx: u64,
    pub(crate) rcx: u64, // +0xb0
    pub(crate) rdx: u64,
    pub(crate) rdi: u64, // +0xc0
    pub(crate) rsi: u64,
    pub(crate) rbp: u64, // +0xd0
    pub(crate) r8: u64,
    pub(crate) r9: u64, // +0xe0
    pub(crate) r10: u64,
    pub(crate) r11: u64, // +0xf0
    pub(crate) r12: u64,
    pub(crate) r13: u64, // +0x100
    pub(crate) r14: u64,
    pub(crate) r15: u64, // +0x110
}

impl Snapshot {
    /// Initializes the vastly empty [`Snapshot`].
    pub(crate) fn new(dir: &mut Directory, snapshot_path: &str) -> Result<Self, uefi::Error> {
        let mut snapshot_file = open_file(dir, snapshot_path)?;

        // Safety: Code is single threaded.
        let size = unsafe { get_file_info(&mut snapshot_file) }?.file_size() as usize;
        let size_in_pages = size_to_pages(size);
        info!("Snapshot size {size:#x}");
        if size_in_pages == 1 || (size % BASE_PAGE_SIZE) != 0 {
            error!("{snapshot_path:?} is not a snapshot file (invalid file size)");
            return Err(uefi::Error::from(uefi::Status::INVALID_PARAMETER));
        }

        // Read 4KB metadata at the end the file.
        let mut page = Page::new();
        read_page_from_snapshot(&mut snapshot_file, &mut page, size_in_pages - 1)?;
        let metadata = unsafe { core::mem::transmute::<Page, SnapshotMetadataRaw>(page) };
        if metadata.magic != SNAPSHOT_SIGNATURE {
            error!("{snapshot_path:?} is not a snapshot file (signature not found)");
            return Err(uefi::Error::from(uefi::Status::INVALID_PARAMETER));
        }

        // Capture physical memory ranges saved in the snapshot.
        let mut memory_ranges: Vec<SnapshotMemoryRange> = Vec::new();
        metadata.memory_ranges.iter().for_each(|range| {
            if range.page_count != 0 {
                debug!(
                    "Memory range: {:#x} - {:#x}",
                    range.page_base,
                    range.page_base + range.page_count * (BASE_PAGE_SIZE as u64)
                );
                memory_ranges.push(range.clone());
            }
        });

        // Allocates the buffer for snapshot memory. Contents will be populated
        // on-demand. No zero initialization as it is very slow (huge memory).
        let memory_size_in_pages = size_in_pages - 1; // do not include the metadata size
        let memory = unsafe { Box::<[Page]>::new_uninit_slice(memory_size_in_pages).assume_init() };

        debug!("{:#x?}", metadata.registers);
        let mut snapshot = Self {
            registers: metadata.registers,
            memory,
            memory_ranges,
            read_bitmap: BitVec::from_elem(memory_size_in_pages, false),
            resolved_page_count: 0,
            snapshot_file,
        };

        // Page-in contents of the guest GDT, as a later VM setting up step needs
        // to read the table and get guest segment related values.
        let pfn = snapshot.registers.gdtr.base as usize >> BASE_PAGE_SHIFT;
        let _ = snapshot.resolve_page(pfn)?;
        Ok(snapshot)
    }

    // Checks whether the given page is captured in the snapshot file.
    fn contains(&self, pfn: usize) -> bool {
        self.memory_ranges.iter().any(|range| {
            let base = (range.page_base >> BASE_PAGE_SHIFT) as usize;
            (base..base + range.page_count as usize).contains(&pfn)
        })
    }

    // Resolves the page that should back the given guest `pfn`.
    fn resolve_page(&mut self, pfn: usize) -> Result<&mut Page, uefi::Error> {
        let page = &mut self.memory[pfn];
        read_page_from_snapshot(&mut self.snapshot_file, page, pfn)?;
        self.read_bitmap.set(pfn, true);
        self.resolved_page_count += 1;
        Ok(page)
    }
}

// Resolves snapshot contents that should back the given guest `pfn` from the
// snapshot file and applies patches as needed.
pub(crate) fn resolve_page_from_snapshot(global: &GlobalState, pfn: usize) -> Option<*const Page> {
    if !global.snapshot().contains(pfn) {
        return None;
    }

    // Locking for modifying `global` is required.
    let mut snapshot = global.snapshot_mut();

    if !snapshot.read_bitmap[pfn] {
        let page = snapshot.resolve_page(pfn).unwrap();
        global.patch_set().apply(pfn, page);
    }

    Some(addr_of!(snapshot.memory[pfn]))
}

// The magic value at the beginning of the metadata page in the snapshot file.
const SNAPSHOT_SIGNATURE: u64 = 0x544F_4853_5041_4E53; // 'SNAPSHOT'

// The maximum number of memory ranges in the snapshot file.
const MAX_MEMORY_DESCRIPTOR_COUNT: usize = 47;

/// The contents of the last 4KB of the snapshot file.
#[derive(Debug)]
#[repr(C, align(4096))]
struct SnapshotMetadataRaw {
    /// The magic value. Must be [`SNAPSHOT_SIGNATURE`]
    magic: u64,
    _padding1: u64,
    /// The ranges of physical memory captured in the snapshot file.
    memory_ranges: [SnapshotMemoryRange; MAX_MEMORY_DESCRIPTOR_COUNT],
    /// The collection of register values stored in the snapshot file.
    registers: SnapshotRegisters,
}
const _: () = assert!(core::mem::size_of::<SnapshotMetadataRaw>() == 0x1000);

/// A range of physical memory captured in the snapshot file.
#[derive(Debug, Clone)]
#[repr(C)]
struct SnapshotMemoryRange {
    page_base: u64,
    page_count: u64,
}
