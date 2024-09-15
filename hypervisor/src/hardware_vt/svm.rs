//! The module containing the [`Svm`] type, which implements the
//! [`hardware_vt::HardwareVt`] trait for AMD processors.
//!
//! The Secure Virtual Machine (SVM) extension implements AMD Virtualization
//! (AMD-V), the hardware assisted virtualization technology on AMD processors.
//!
//! All references to external resources (denoted with "See:") refers to
//! "AMD64 Architecture Programmer’s Manual Volume 2: System Programming"
//! Revision 3.40 (January 2023) at
//! <https://developer.amd.com/resources/developer-guides-manuals/> unless
//! otherwise stated.

use super::{
    get_segment_descriptor_value, get_segment_limit, GuestRegisters,
    NestedPagingStructureEntryFlags, NestedPagingStructureEntryType, VmExitReason,
};
use crate::{
    hardware_vt::{self, ExceptionQualification, GuestException, NestedPageFaultQualification},
    snapshot::Snapshot,
    x86_instructions::{rdmsr, wrmsr},
};
use alloc::boxed::Box;
use core::{
    arch::global_asm,
    ptr::{addr_of, addr_of_mut},
};
use x86::{current::paging::BASE_PAGE_SHIFT, irq};

/// SVM-specific data to represent a guest.
#[derive(derivative::Derivative)]
#[derivative(Debug, Default)]
pub(crate) struct Svm {
    vmcb: Box<Vmcb>,
    #[derivative(Debug = "ignore")]
    host_state: Box<HostStateArea>,
    registers: GuestRegisters,
}

impl hardware_vt::HardwareVt for Svm {
    /// Enables SVM on the current processor.
    fn enable(&mut self) {
        const EFER_SVME: u64 = 1 << 12;

        // Enable SVM. We assume the processor is compatible with this.
        // See: 15.4 Enabling SVM
        wrmsr(x86::msr::IA32_EFER, rdmsr(x86::msr::IA32_EFER) | EFER_SVME);
    }

    /// Configures SVM. We intercept #BP, #UD, #PF, external interrupt, the
    /// PAUSE instruction, shutdown, and enable nested paging.
    fn initialize(&mut self, nested_pml4_addr: u64) {
        const SVM_INTERCEPT_MISC1_INTR: u32 = 1 << 0;
        const SVM_INTERCEPT_MISC1_PAUSE: u32 = 1 << 23;
        const SVM_INTERCEPT_MISC1_SHUTDOWN: u32 = 1 << 31;
        const SVM_INTERCEPT_MISC2_VMRUN: u32 = 1 << 0;
        const SVM_NP_ENABLE_NP_ENABLE: u64 = 1 << 0;
        const SVM_MSR_VM_HSAVE_PA: u32 = 0xc001_0117;

        // Need to specify the address of the host state-save area before executing
        // the VMRUN instruction. The host state-save area is where the processor
        // saves the host (ie, current) register values on execution of `VMRUN`.
        //
        // "The VMRUN instruction saves some host processor state information in
        //  the host state-save area in main memory at the physical address
        //  specified in the VM_HSAVE_PA MSR".
        // See: 15.5.1 Basic Operation
        wrmsr(SVM_MSR_VM_HSAVE_PA, addr_of!(*self.host_state) as u64);

        // Intercept external interrupts, the PAUSE instruction and shutdown.
        // Additionally, intercept the VMRUN instruction which is a HW requirement.
        //
        // We intercept external interrupts and PAUSE as an attempt to gain control
        // even if the guest is in an infinite loop, although this is not a perfect
        // solution. PAUSE causes #VMEXIT when it is executed u16::MAX times.
        //
        // We also intercept shutdown to prevent the guest from causing system
        // reset. We want to abort the guest instead. Note that, on Intel, event
        // that would normally cause system reset, eg, triple fault, are
        // intercepted by default.
        //
        // See: 15.13.1 INTR Intercept
        // See: 15.14.3 Shutdown Intercept
        // See: 15.14.4 Pause Intercept Filtering
        self.vmcb.control_area.intercept_misc1 =
            SVM_INTERCEPT_MISC1_INTR | SVM_INTERCEPT_MISC1_PAUSE | SVM_INTERCEPT_MISC1_SHUTDOWN;
        self.vmcb.control_area.intercept_misc2 = SVM_INTERCEPT_MISC2_VMRUN;
        self.vmcb.control_area.pause_filter_count = u16::MAX;

        // Address Space Identifier (ASID) is useful when the given logical processor
        // runs more than one guests. We do not but still need to set non-zero value.
        // See: 15.16 TLB Control
        self.vmcb.control_area.guest_asid = 1;

        // Enable nested paging. This is done by:
        // - Setting the NP_ENABLE bit in VMCB, and
        // - Setting the base address of the nested PML4
        //
        // See: 15.25.3 Enabling Nested Paging
        self.vmcb.control_area.np_enable = SVM_NP_ENABLE_NP_ENABLE;
        self.vmcb.control_area.ncr3 = nested_pml4_addr;

        // Intercept #BP, #UD, #PF.
        // See: 15.12 Exception Intercepts
        self.vmcb.control_area.intercept_exception = (1u32 << irq::BREAKPOINT_VECTOR)
            | (1u32 << irq::INVALID_OPCODE_VECTOR)
            | (1u32 << irq::PAGE_FAULT_VECTOR);
    }

    /// Configures the guest states based on the snapshot.
    fn revert_registers(&mut self, snapshot: &Snapshot) {
        const EFER_SVME: u64 = 1 << 12;
        const CR4_VMXE: u64 = 1 << 13;

        let registers = &snapshot.registers;

        // Initialize guest registers from the snapshot through VMCB. Note that:
        // - SVME bit has to be set in the EFER.
        //   - See: 15.5.1 Basic Operation
        //     - and subsection "Canonicalization and Consistency Checks."
        // - VMXE bit must be cleared in CR4, because it does not exist on AMD. This is
        //   required to consume a snapshot taken on the Intel platform.
        let guest_gdt_pfn = registers.gdtr.base as usize >> BASE_PAGE_SHIFT;
        let guest_gdt = addr_of!(snapshot.memory[guest_gdt_pfn]) as u64;
        self.vmcb.state_save_area.es_selector = registers.es;
        self.vmcb.state_save_area.cs_selector = registers.cs;
        self.vmcb.state_save_area.ss_selector = registers.ss;
        self.vmcb.state_save_area.ds_selector = registers.ds;
        self.vmcb.state_save_area.fs_selector = registers.fs;
        self.vmcb.state_save_area.gs_selector = registers.gs;
        self.vmcb.state_save_area.tr_selector = registers.tr;
        self.vmcb.state_save_area.ldtr_selector = registers.ldtr;
        self.vmcb.state_save_area.es_attrib = get_segment_access_right(guest_gdt, registers.es);
        self.vmcb.state_save_area.cs_attrib = get_segment_access_right(guest_gdt, registers.cs);
        self.vmcb.state_save_area.ss_attrib = get_segment_access_right(guest_gdt, registers.ss);
        self.vmcb.state_save_area.ds_attrib = get_segment_access_right(guest_gdt, registers.ds);
        self.vmcb.state_save_area.fs_attrib = get_segment_access_right(guest_gdt, registers.fs);
        self.vmcb.state_save_area.gs_attrib = get_segment_access_right(guest_gdt, registers.gs);
        self.vmcb.state_save_area.tr_attrib = get_segment_access_right(guest_gdt, registers.tr);
        self.vmcb.state_save_area.ldtr_attrib = get_segment_access_right(guest_gdt, registers.ldtr);
        self.vmcb.state_save_area.es_limit = get_segment_limit(guest_gdt, registers.es);
        self.vmcb.state_save_area.cs_limit = get_segment_limit(guest_gdt, registers.cs);
        self.vmcb.state_save_area.ss_limit = get_segment_limit(guest_gdt, registers.ss);
        self.vmcb.state_save_area.ds_limit = get_segment_limit(guest_gdt, registers.ds);
        self.vmcb.state_save_area.fs_limit = get_segment_limit(guest_gdt, registers.fs);
        self.vmcb.state_save_area.gs_limit = get_segment_limit(guest_gdt, registers.gs);
        self.vmcb.state_save_area.tr_limit = get_segment_limit(guest_gdt, registers.tr);
        self.vmcb.state_save_area.ldtr_limit = get_segment_limit(guest_gdt, registers.ldtr);
        self.vmcb.state_save_area.fs_base = registers.fs_base;
        self.vmcb.state_save_area.gs_base = registers.gs_base;
        self.vmcb.state_save_area.tr_base = registers.tr_base;
        self.vmcb.state_save_area.ldtr_base = registers.ldtr_base;
        self.vmcb.state_save_area.gdtr_base = registers.gdtr.base as u64;
        self.vmcb.state_save_area.gdtr_limit = u32::from(registers.gdtr.limit);
        self.vmcb.state_save_area.idtr_base = registers.idtr.base as u64;
        self.vmcb.state_save_area.idtr_limit = u32::from(registers.idtr.limit);
        self.vmcb.state_save_area.sysenter_cs = registers.sysenter_cs;
        self.vmcb.state_save_area.sysenter_esp = registers.sysenter_esp;
        self.vmcb.state_save_area.sysenter_eip = registers.sysenter_eip;
        self.vmcb.state_save_area.efer = registers.efer | EFER_SVME;
        self.vmcb.state_save_area.cr0 = registers.cr0;
        self.vmcb.state_save_area.cr3 = registers.cr3;
        self.vmcb.state_save_area.cr4 = registers.cr4 & !CR4_VMXE;
        self.vmcb.state_save_area.rip = registers.rip;
        self.vmcb.state_save_area.rsp = registers.rsp;
        self.vmcb.state_save_area.rflags = registers.rflags;
        self.vmcb.state_save_area.rax = registers.rax;
        self.vmcb.state_save_area.gpat = rdmsr(x86::msr::IA32_PAT); // FIXME; use snapshot

        // Some registers are not managed by VMCB and needed to be manually saved
        // and loaded by software. General purpose registers are such examples.
        //
        // Note that RAX is managed within VMCB. See `StateSaveArea` and just above.
        self.registers.rbx = registers.rbx;
        self.registers.rcx = registers.rcx;
        self.registers.rdx = registers.rdx;
        self.registers.rdi = registers.rdi;
        self.registers.rsi = registers.rsi;
        self.registers.rbp = registers.rbp;
        self.registers.r8 = registers.r8;
        self.registers.r9 = registers.r9;
        self.registers.r10 = registers.r10;
        self.registers.r11 = registers.r11;
        self.registers.r12 = registers.r12;
        self.registers.r13 = registers.r13;
        self.registers.r14 = registers.r14;
        self.registers.r15 = registers.r15;
    }

    /// Updates the guest states to have the guest use input data.
    fn adjust_registers(&mut self, input_addr: u64, input_size: u64) {
        // For the snapshot being used for testing, we know RDI points to the
        // address of the buffer to be parsed, and RSI contains the size of it.
        self.registers.rdi = input_addr;
        self.registers.rsi = input_size;
    }

    /// Executes the guest until it triggers #VMEXIT.
    fn run(&mut self) -> VmExitReason {
        const VMEXIT_EXCP0: u64 = 0x40;
        const VMEXIT_EXCP31: u64 = 0x5f;
        const VMEXIT_INTR: u64 = 0x60;
        const VMEXIT_PAUSE: u64 = 0x77;
        const VMEXIT_RESET: u64 = 0x7f;
        const VMEXIT_NPF: u64 = 0x400;

        // Run the VM until the #VMEXIT occurs.
        unsafe { run_vm_svm(&mut self.registers, addr_of_mut!(*self.vmcb)) };

        // #VMEXIT occurred. Copy the guest register values from VMCB so that
        // `self.registers` is complete and up to date.
        self.registers.rax = self.vmcb.state_save_area.rax;
        self.registers.rip = self.vmcb.state_save_area.rip;
        self.registers.rsp = self.vmcb.state_save_area.rsp;
        self.registers.rflags = self.vmcb.state_save_area.rflags;

        // We might have requested flushing TLB. Clear the request.
        self.vmcb.control_area.tlb_control = 0;

        // Handle #VMEXIT by translating it to the `VmExitReason` type.
        //
        // "On #VMEXIT, the processor:
        //  (...)
        //  - Saves the reason for exiting the guest in the VMCB's EXITCODE field."
        // See: 15.6 #VMEXIT
        //
        // For the list of possible exit codes,
        // See: Appendix C SVM Intercept Exit Codes
        match self.vmcb.control_area.exit_code {
            // See: 15.12 Exception Intercepts
            VMEXIT_EXCP0..=VMEXIT_EXCP31 => VmExitReason::Exception(ExceptionQualification {
                rip: self.registers.rip,
                exception_code: GuestException::try_from(
                    (self.vmcb.control_area.exit_code - VMEXIT_EXCP0) as u8,
                )
                .unwrap(),
            }),
            // See: 15.25.6 Nested versus Guest Page Faults, Fault Ordering
            VMEXIT_NPF => VmExitReason::NestedPageFault(NestedPageFaultQualification {
                rip: self.registers.rip,
                gpa: self.vmcb.control_area.exit_info2,
                missing_translation: (self.vmcb.control_area.exit_info1 & 0b1) == 0,
                write_access: (self.vmcb.control_area.exit_info1 & 0b10) != 0,
            }),
            // See: 15.13.1 INTR Intercept
            // See: 15.14.4 Pause Intercept Filtering
            VMEXIT_INTR | VMEXIT_PAUSE => VmExitReason::ExternalInterruptOrPause,
            // See: 15.14.3 Shutdown Intercept
            VMEXIT_RESET => VmExitReason::Shutdown(self.vmcb.control_area.exit_code),
            // Anything else.
            _ => VmExitReason::Unexpected(self.vmcb.control_area.exit_code),
        }
    }

    /// Invalidates caches of the nested paging structures.
    fn invalidate_caches(&mut self) {
        // Flushes this guest's TLB entries.
        // See: Table 15-9. TLB Control Byte Encodings
        self.vmcb.control_area.tlb_control = 0b11;
    }

    /// Gets a flag value to be set to nested paging structure entries for the
    /// given entry types (eg, permissions).
    fn nps_entry_flags(
        &self,
        entry_type: NestedPagingStructureEntryType,
    ) -> NestedPagingStructureEntryFlags {
        // SVM uses the exact same layout as the standard paging structure entries
        // for nested paging structure entries. We also assume leaving the PWT, PCD, and
        // PAT bits zero in the entry results in the write-back memory type. Thus,
        // `NestedPagingStructureEntryType::Wb*` types results in the same permission
        // bits as `NestedPagingStructureEntryType::*` types.
        match entry_type {
            // Valid, Writeable, User
            NestedPagingStructureEntryType::Rwx | NestedPagingStructureEntryType::RwxWriteBack => {
                NestedPagingStructureEntryFlags {
                    permission: 0b111,
                    memory_type: 0,
                }
            }
            // Valid, NON writable, User
            NestedPagingStructureEntryType::RxWriteBack => NestedPagingStructureEntryFlags {
                permission: 0b101,
                memory_type: 0,
            },
        }
    }
}

impl Svm {
    pub(crate) fn new() -> Self {
        let vmcb = unsafe { Box::<Vmcb>::new_zeroed().assume_init() };
        let host_state = unsafe { Box::<HostStateArea>::new_zeroed().assume_init() };
        Self {
            vmcb,
            host_state,
            ..Default::default()
        }
    }
}

/// The virtual machine control block (VMCB), which describes a virtual machine
/// (guest) to be executed.
///
/// See: Appendix B Layout of VMCB
#[derive(Debug, Default)]
#[repr(C, align(4096))]
struct Vmcb {
    control_area: ControlArea,
    state_save_area: StateSaveArea,
}
const _: () = assert!(size_of::<Vmcb>() == 0x1000);

/// The "metadata" area where we can specify what operations to intercept and
/// can read details of #VMEXIT.
///
/// See: Table B-1. VMCB Layout, Control Area
#[derive(derivative::Derivative)]
#[derivative(Debug, Default)]
#[repr(C)]
struct ControlArea {
    intercept_cr_read: u16,   // +0x000
    intercept_cr_write: u16,  // +0x002
    intercept_dr_read: u16,   // +0x004
    intercept_dr_write: u16,  // +0x006
    intercept_exception: u32, // +0x008
    intercept_misc1: u32,     // +0x00c
    intercept_misc2: u32,     // +0x010
    intercept_misc3: u32,     // +0x014
    #[derivative(Debug = "ignore", Default(value = "[0; 36]"))]
    _padding1: [u8; 0x03c - 0x018], // +0x018
    pause_filter_threshold: u16, // +0x03c
    pause_filter_count: u16,  // +0x03e
    iopm_base_pa: u64,        // +0x040
    msrpm_base_pa: u64,       // +0x048
    tsc_offset: u64,          // +0x050
    guest_asid: u32,          // +0x058
    tlb_control: u32,         // +0x05c
    vintr: u64,               // +0x060
    interrupt_shadow: u64,    // +0x068
    exit_code: u64,           // +0x070
    exit_info1: u64,          // +0x078
    exit_info2: u64,          // +0x080
    exit_int_info: u64,       // +0x088
    np_enable: u64,           // +0x090
    avic_apic_bar: u64,       // +0x098
    guest_pa_pf_ghcb: u64,    // +0x0a0
    event_inj: u64,           // +0x0a8
    ncr3: u64,                // +0x0b0
    lbr_virtualization_enable: u64, // +0x0b8
    vmcb_clean: u64,          // +0x0c0
    nrip: u64,                // +0x0c8
    num_of_bytes_fetched: u8, // +0x0d0
    guest_instruction_bytes: [u8; 15], // +0x0d1
    avic_apic_backing_page_pointer: u64, // +0x0e0
    #[derivative(Debug = "ignore")]
    _padding2: u64, // +0x0e8
    avic_logical_table_pointer: u64, // +0x0f0
    avic_physical_table_pointer: u64, // +0x0f8
    #[derivative(Debug = "ignore")]
    _padding3: u64, // +0x100
    vmcb_save_state_pointer: u64, // +0x108
    #[derivative(Debug = "ignore", Default(value = "[0; 720]"))]
    _padding4: [u8; 0x3e0 - 0x110], // +0x110
    reserved_for_host: [u8; 0x20], // +0x3e0
}
const _: () = assert!(size_of::<ControlArea>() == 0x400);

/// The ares to specify and read guest register values.
///
/// See: Table B-2. VMCB Layout, State Save Area
#[derive(derivative::Derivative)]
#[derivative(Debug, Default)]
#[repr(C)]
struct StateSaveArea {
    es_selector: u16,   // +0x000
    es_attrib: u16,     // +0x002
    es_limit: u32,      // +0x004
    es_base: u64,       // +0x008
    cs_selector: u16,   // +0x010
    cs_attrib: u16,     // +0x012
    cs_limit: u32,      // +0x014
    cs_base: u64,       // +0x018
    ss_selector: u16,   // +0x020
    ss_attrib: u16,     // +0x022
    ss_limit: u32,      // +0x024
    ss_base: u64,       // +0x028
    ds_selector: u16,   // +0x030
    ds_attrib: u16,     // +0x032
    ds_limit: u32,      // +0x034
    ds_base: u64,       // +0x038
    fs_selector: u16,   // +0x040
    fs_attrib: u16,     // +0x042
    fs_limit: u32,      // +0x044
    fs_base: u64,       // +0x048
    gs_selector: u16,   // +0x050
    gs_attrib: u16,     // +0x052
    gs_limit: u32,      // +0x054
    gs_base: u64,       // +0x058
    gdtr_selector: u16, // +0x060
    gdtr_attrib: u16,   // +0x062
    gdtr_limit: u32,    // +0x064
    gdtr_base: u64,     // +0x068
    ldtr_selector: u16, // +0x070
    ldtr_attrib: u16,   // +0x072
    ldtr_limit: u32,    // +0x074
    ldtr_base: u64,     // +0x078
    idtr_selector: u16, // +0x080
    idtr_attrib: u16,   // +0x082
    idtr_limit: u32,    // +0x084
    idtr_base: u64,     // +0x088
    tr_selector: u16,   // +0x090
    tr_attrib: u16,     // +0x092
    tr_limit: u32,      // +0x094
    tr_base: u64,       // +0x098
    #[derivative(Debug = "ignore", Default(value = "[0; 43]"))]
    _padding1: [u8; 0x0cb - 0x0a0], // +0x0a0
    cpl: u8,            // +0x0cb
    #[derivative(Debug = "ignore")]
    _padding2: u32, // +0x0cc
    efer: u64,          // +0x0d0
    #[derivative(Debug = "ignore", Default(value = "[0; 112]"))]
    _padding3: [u8; 0x148 - 0x0d8], // +0x0d8
    cr4: u64,           // +0x148
    cr3: u64,           // +0x150
    cr0: u64,           // +0x158
    dr7: u64,           // +0x160
    dr6: u64,           // +0x168
    rflags: u64,        // +0x170
    rip: u64,           // +0x178
    #[derivative(Debug = "ignore", Default(value = "[0; 88]"))]
    _padding4: [u8; 0x1d8 - 0x180], // +0x180
    rsp: u64,           // +0x1d8
    s_cet: u64,         // +0x1e0
    ssp: u64,           // +0x1e8
    isst_addr: u64,     // +0x1f0
    rax: u64,           // +0x1f8
    star: u64,          // +0x200
    lstar: u64,         // +0x208
    cstar: u64,         // +0x210
    sf_mask: u64,       // +0x218
    kernel_gs_base: u64, // +0x220
    sysenter_cs: u64,   // +0x228
    sysenter_esp: u64,  // +0x230
    sysenter_eip: u64,  // +0x238
    cr2: u64,           // +0x240
    #[derivative(Debug = "ignore", Default(value = "[0; 32]"))]
    _padding5: [u8; 0x268 - 0x248], // +0x248
    gpat: u64,          // +0x268
    dbg_ctl: u64,       // +0x270
    br_from: u64,       // +0x278
    br_to: u64,         // +0x280
    last_excep_from: u64, // +0x288
    last_excep_to: u64, // +0x290
    #[derivative(Debug = "ignore", Default(value = "[0; 71]"))]
    _padding6: [u8; 0x2df - 0x298], // +0x298
    spec_ctl: u64,      // +0x2e0
}
const _: () = assert!(size_of::<StateSaveArea>() == 0x2e8);

/// 4KB block of memory where the host state is saved to on VMRUN and loaded
/// from on #VMEXIT.
///
/// See: 15.30.4 VM_HSAVE_PA MSR (C001_0117h)
// doc_markdown: clippy confused with "VM_HSAVE_PA"
#[allow(clippy::doc_markdown)]
#[repr(C, align(4096))]
struct HostStateArea([u8; 0x1000]);
const _: () = assert!(size_of::<HostStateArea>() == 0x1000);

impl Default for HostStateArea {
    fn default() -> Self {
        Self([0; 4096])
    }
}

extern "efiapi" {
    /// Runs the guest until #VMEXIT occurs.
    fn run_vm_svm(registers: &mut GuestRegisters, guest_vmcb_pa: *mut Vmcb);
}
global_asm!(include_str!("svm_run_vm.S"));

/// Returns the access rights of the given segment for SVM.
fn get_segment_access_right(table_base: u64, selector: u16) -> u16 {
    let descriptor_value = get_segment_descriptor_value(table_base, selector);

    // First, get the AVL, L, D/B and G bits, while excluding the "Seg. Limit 19:16"
    // bits. Then, get the Type, S, DPL and P bits. Finally, return those bits
    // without the "Seg. Limit 19:16" bits.
    // See: Figure 3-8. Segment Descriptor
    let ar = (descriptor_value >> 40) as u16;
    let upper_ar = (ar >> 4) & 0b1111_0000_0000;
    let lower_ar = ar & 0b1111_1111;
    lower_ar | upper_ar
}
