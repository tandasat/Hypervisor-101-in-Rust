//! The module containing vendor agnostic representation of HW VT
//! (hardware-assisted virtualization technology) related definitions.

pub(crate) mod svm;
pub(crate) mod vmx;

use crate::snapshot::Snapshot;
use bitfield::bitfield;
use core::fmt;
use x86::{
    current::paging::{BASE_PAGE_SHIFT, PAGE_SIZE_ENTRIES},
    irq,
};

/// This trait represents an interface to enable HW VT, setup and run a single
/// virtual machine instance on the current processor.
pub(crate) trait HardwareVt: fmt::Debug {
    /// Enables HW VT on the current processor. It has to be called exactly once
    /// before calling any other method.
    fn enable(&mut self);

    /// Configures HW VT such as enabling nested paging and exception
    /// interception.
    fn initialize(&mut self, nested_pml4_addr: u64);

    /// Configures the guest states based on the snapshot.
    fn revert_registers(&mut self, snapshot: &Snapshot);

    /// Updates the guest states to make the guest use input data.
    fn adjust_registers(&mut self, input_addr: u64, input_size: u64);

    /// Executes the guest until it triggers VM exit.
    fn run(&mut self) -> VmExitReason;

    /// Invalidates caches of the nested paging structures.
    fn invalidate_caches(&mut self);

    /// Gets a flag value to be set to nested paging structure entries for the
    /// given entry types (eg, permissions).
    fn nps_entry_flags(
        &self,
        entry_type: NestedPagingStructureEntryType,
    ) -> NestedPagingStructureEntryFlags;
}

/// Reasons of VM exit.
pub(crate) enum VmExitReason {
    /// An address translation failure with nested paging. Contains a guest
    /// physical address that failed translation and whether the access was
    /// write access.
    NestedPageFault(NestedPageFaultQualification),

    /// An exception happened. Contains an exception code.
    Exception(ExceptionQualification),

    /// An external interrupt occurred, or `PAUSE` was executed more than
    /// certain times.
    ExternalInterruptOrPause,

    /// The guest ran long enough to use up its time slice.
    TimerExpiration,

    /// The logical processor entered the shutdown state, eg, triple fault.
    Shutdown(u64),

    /// An unhandled VM exit happened. Contains a vendor specific VM exit code.
    Unexpected(u64),
}

/// Details of the cause of nested page fault.
#[derive(Debug)]
pub(crate) struct NestedPageFaultQualification {
    #[allow(unused)]
    pub(crate) rip: u64,
    pub(crate) gpa: u64,
    pub(crate) missing_translation: bool,
    pub(crate) write_access: bool,
}

pub(crate) struct ExceptionQualification {
    pub(crate) rip: u64,
    pub(crate) exception_code: GuestException,
}

/// The cause of guest exception.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum GuestException {
    BreakPoint,
    InvalidOpcode,
    PageFault,
}

impl TryFrom<u8> for GuestException {
    type Error = &'static str;

    fn try_from(vector: u8) -> Result<Self, Self::Error> {
        match vector {
            irq::BREAKPOINT_VECTOR => Ok(GuestException::BreakPoint),
            irq::INVALID_OPCODE_VECTOR => Ok(GuestException::InvalidOpcode),
            irq::PAGE_FAULT_VECTOR => Ok(GuestException::PageFault),
            _ => Err("Vector of the exception that is not intercepted"),
        }
    }
}

/// Permissions and memory types to be specified for nested paging structure
/// entries.
pub(crate) enum NestedPagingStructureEntryType {
    /// Readable, writable, executable.
    Rwx,

    /// Readable, writable, executable, with the write-back memory type.
    RwxWriteBack,

    /// Readable, NON writable, executable, with the write-back memory type.
    RxWriteBack,
}

/// The values used to initialize [`NestedPagingStructureEntry`].
#[derive(Clone, Copy)]
pub(crate) struct NestedPagingStructureEntryFlags {
    pub(crate) permission: u8,
    pub(crate) memory_type: u8,
}

/// The collection of the guest general purpose register values.
#[derive(Debug, Default)]
#[repr(C)]
struct GuestRegisters {
    pub(crate) rax: u64,
    pub(crate) rbx: u64,
    pub(crate) rcx: u64,
    pub(crate) rdx: u64,
    pub(crate) rdi: u64,
    pub(crate) rsi: u64,
    pub(crate) rbp: u64,
    pub(crate) r8: u64,
    pub(crate) r9: u64,
    pub(crate) r10: u64,
    pub(crate) r11: u64,
    pub(crate) r12: u64,
    pub(crate) r13: u64,
    pub(crate) r14: u64,
    pub(crate) r15: u64,
    pub(crate) rip: u64,
    pub(crate) rsp: u64,
    pub(crate) rflags: u64,
}

/// A single nested paging structure.
///
/// This is a extended page table on Intel and a nested page table on AMD. The
/// details of the layout are not represented in this structure so that it may
/// be used for any the structures (PML4, PDPT, PD and PT) across platforms.
#[derive(Clone, Copy, Debug)]
#[repr(C, align(4096))]
pub(crate) struct NestedPagingStructure {
    /// An array of extended page table entry (8 bytes, 512 entries)
    pub(crate) entries: [NestedPagingStructureEntry; PAGE_SIZE_ENTRIES],
}
const _: () = assert!(size_of::<NestedPagingStructure>() == 0x1000);

bitfield! {
    /// Platform independent representation of a nested paging structure entry.
    ///
    /// Because it is platform independent, the layout is not exactly correct.
    /// For example, bit 5:3 `memory_type` exists only on Intel. On AMD, those are
    /// other bits and we set zeros.
    /*
         66665 5     1 110000 000 000
         32109 8.....2 109876 543 210
        +-----+-------+------+---+---+
        |xxxxx|  PFN  |xxxxxx| M | P |
        +-----+-------+------+---+---+
    */
    #[derive(Clone, Copy)]
    pub struct NestedPagingStructureEntry(u64);
    impl Debug;
    permission, set_permission: 2, 0;
    memory_type, set_memory_type: 5, 3;
    flags1, _: 11, 6;
    pub pfn, set_pfn: 58, 12;
    flags2, _: 63, 59;
}

impl NestedPagingStructureEntry {
    /// Returns the next nested paging structures.
    pub(crate) fn next_table_mut(&mut self) -> &mut NestedPagingStructure {
        let next_table_addr = self.pfn() << BASE_PAGE_SHIFT;
        assert!(next_table_addr != 0);
        let next_table_ptr = next_table_addr as *mut NestedPagingStructure;
        unsafe { next_table_ptr.as_mut() }.unwrap()
    }

    /// Sets the address to the next nested paging structure or final physical
    /// address with permissions specified by `flags`.
    pub(crate) fn set_translation(&mut self, pa: u64, flags: NestedPagingStructureEntryFlags) {
        self.set_pfn(pa >> BASE_PAGE_SHIFT);
        self.set_permission(u64::from(flags.permission));
        self.set_memory_type(u64::from(flags.memory_type));
    }
}

/// Returns the segment descriptor casted as a 64bit integer for the given
/// selector.
fn get_segment_descriptor_value(table_base: u64, selector: u16) -> u64 {
    let sel = x86::segmentation::SegmentSelector::from_raw(selector);
    let descriptor_addr = table_base + u64::from(sel.index() * 8);
    let ptr = descriptor_addr as *const u64;
    unsafe { *ptr }
}

/// Returns the limit of the given segment.
fn get_segment_limit(table_base: u64, selector: u16) -> u32 {
    let sel = x86::segmentation::SegmentSelector::from_raw(selector);
    if sel.index() == 0 && (sel.bits() >> 2) == 0 {
        return 0; // unusable
    }
    let descriptor_value = get_segment_descriptor_value(table_base, selector);
    let limit_low = descriptor_value & 0xffff;
    let limit_high = (descriptor_value >> (32 + 16)) & 0xF;
    let mut limit = limit_low | (limit_high << 16);
    if ((descriptor_value >> (32 + 23)) & 0x01) != 0 {
        limit = ((limit + 1) << BASE_PAGE_SHIFT) - 1;
    }
    limit as u32
}
