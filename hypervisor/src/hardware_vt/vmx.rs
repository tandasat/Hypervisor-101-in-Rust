//! The module containing the [`Vmx`] type, which implements the
//! [`hardware_vt::HardwareVt`] trait for Intel processors.
//!
//! The virtual-machine extensions (VMX) implements Intel Virtualization
//! Technology (VT-x), the hardware assisted virtualization technology on Intel
//! processors.
//!
//! All references to external resources (denoted with "See:") refers to
//! "Intel 64 and IA-32 Architectures Software Developer’s Manual Volume 3"
//! Revision 78 (December 2022) at <https://www.intel.com/sdm/> unless otherwise
//! stated.

use super::{
    get_segment_descriptor_value, get_segment_limit, GuestRegisters,
    NestedPagingStructureEntryFlags, NestedPagingStructureEntryType, VmExitReason,
};
use crate::{
    config::GUEST_EXEC_TIMEOUT_IN_TSC,
    hardware_vt::{self, ExceptionQualification, GuestException, NestedPageFaultQualification},
    snapshot::Snapshot,
    x86_instructions::{cr0, cr0_write, cr3, cr4, cr4_write, rdmsr, sgdt, sidt, wrmsr},
};
use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    vec::Vec,
};
use core::{
    arch::{asm, global_asm},
    fmt,
    ptr::addr_of,
};
use log::{debug, warn};
use x86::{
    controlregs::{Cr0, Cr4},
    current::{paging::BASE_PAGE_SHIFT, rflags::RFlags},
    dtables::DescriptorTablePointer,
    irq,
    segmentation::{
        BuildDescriptor, Descriptor, DescriptorBuilder, GateDescriptorBuilder, SegmentSelector,
    },
    vmx::vmcs,
};

/// VMX-specific data to represent a guest.
#[derive(derivative::Derivative)]
#[derivative(Debug, Default)]
pub(crate) struct Vmx {
    #[derivative(Debug = "ignore")]
    vmxon_region: Box<Vmxon>,
    vmcs_region: Box<Vmcs>,
    #[derivative(Debug = "ignore")]
    host_gdt: HostGdt,
    registers: GuestRegisters,
    /// Whether [`Vmx::vmcs_region`] is already in the launched state.
    launched: bool,
    /// The scale to convert TSC into the unit used for VMX-preemption timer.
    /// If VMX-preemption timer is not supported, None.
    timer_scale: Option<u64>,
}

impl hardware_vt::HardwareVt for Vmx {
    /// Enables VMX operation on the current processor.
    fn enable(&mut self) {
        // Enable VMX, which allows execution of the VMXON instruction.
        //
        // "Before system software can enter VMX operation, it enables VMX by
        //  setting CR4.VMXE[bit 13] = 1."
        // See: 24.7 ENABLING AND ENTERING VMX OPERATION
        cr4_write(cr4() | Cr4::CR4_ENABLE_VMX);

        // Prepare for entering VMX operation by executing the VMXON instruction.
        // To enter VMX operation, several requirements must be met or the
        // instruction fails. We assume all those conditions are already satisfied
        // except ones with the IA32_FEATURE_CONTROL MSR and CR0. Let us fix them
        // up. For details of the requirements,
        // See: VMXON-Enter VMX Operation
        //
        // "VMXON is also controlled by the IA32_FEATURE_CONTROL MSR (...)."
        // See: 24.7 ENABLING AND ENTERING VMX OPERATION
        // "In VMX operation, processors may fix certain bits in CR0 and CR4 to
        //  specific values and not support other values. VMXON fails if any of
        //  these bits contains an unsupported value"
        // See: 24.8 RESTRICTIONS ON VMX OPERATION
        adjust_feature_control_msr();
        adjust_cr0();

        // Execute the VMXON instruction. This instruction requires 4KB of a
        // region called "VMXON region" and that part of the region is initialized
        // with the VMCS revision identifier, which can be read from the
        // IA32_VMX_BASIC MSR.
        //
        // Successful execution of it puts the processor into the operation mode
        // called "VMX root operation" (ie, host-mode) allowing the use of the
        // other VMX instructions.
        //
        // "Before executing VMXON, software should write the VMCS revision
        //  identifier (see Section 25.2) to the VMXON region."
        // See: 25.11.5 VMXON Region
        //
        // "Software can discover the VMCS revision identifier that a processor
        //  uses by reading the VMX capability MSR IA32_VMX_BASIC (see Appendix A.1)."
        // See: 25.2 FORMAT OF THE VMCS REGION
        let revision_id = rdmsr(x86::msr::IA32_VMX_BASIC) as u32;
        self.vmxon_region.revision_id = revision_id;
        vmxon(&mut self.vmxon_region);
    }

    /// Configures VMX. We intercept #BP, #UD, #PF, enable VMX-preemption timer
    /// and extended page tables.
    fn initialize(&mut self, nested_pml4_addr: u64) {
        const IA32_VMX_PROCBASED_CTLS_ACTIVATE_SECONDARY_CONTROLS_FLAG: u64 = 1 << 31;
        const IA32_VMX_EXIT_CTLS_HOST_ADDRESS_SPACE_SIZE_FLAG: u64 = 1 << 9;
        const IA32_VMX_ENTRY_CTLS_IA32E_MODE_GUEST_FLAG: u64 = 1 << 9;
        const IA32_VMX_PROCBASED_CTLS2_ENABLE_EPT_FLAG: u64 = 1 << 1;
        const EPT_POINTER_MEMORY_TYPE_WRITE_BACK: u64 = 6 /* << 0 */;
        const EPT_POINTER_PAGE_WALK_LENGTH_4: u64 = 3 << 3;

        // The processor is now in VMX root operation. This means that the
        // processor can execute other VMX instructions and almost ready for
        // configuring a VMCS with the VMREAD and VMWRITE instructions. Before
        // doing so, we need to make a VMCS "clear", "active" and "current".
        // Otherwise, the VMREAD and VMWRITE instructions do not know which VMCS
        // to operate on and fail.
        //
        // For visualization of VMCS state transitions,
        // See: Figure 25-1. States of VMCS X

        // Firstly, "clear" the VMCS using the VMCLEAR instruction. Effect of this
        // instruction is not directly observable by software; it is implementation
        // specific.
        //
        // "the VMCLEAR instruction initializes any implementation-specific
        //  information in the VMCS region referenced by its operand. (...),
        //  software should execute VMCLEAR on a VMCS region before making the
        //  corresponding VMCS active with VMPTRLD for the first time."
        // See: 25.11.3 Initializing a VMCS
        vmclear(&mut self.vmcs_region);

        // Then, make it "active" and "current" using the VMPTRLD instruction.
        // This instruction requires that the VMCS revision identifier of the
        // VMCS is initialized.
        //
        // "Software should write the VMCS revision identifier to the VMCS region
        //  before using that region for a VMCS. (...) VMPTRLD fails if (...) a
        //  VMCS region whose VMCS revision identifier differs from that used by
        //  the processor."
        // See: 25.2 FORMAT OF THE VMCS REGION
        self.vmcs_region.revision_id = self.vmxon_region.revision_id;
        vmptrld(&mut self.vmcs_region);

        // The processor now has an associated VMCS (called a "current VMCS") and
        // is able to execute the VMREAD and VMWRITE instructions. Let us program
        // the VMCS.

        // Host-State Fields. Largely we copy the current register values since
        // we are going to run as a hypervisor. Exceptions are:
        // - RIP/RSP: we set them within `run_vm_vmx`.
        // - GDT related: the current register values are not compatible as the host
        //   state. We make our own host GDT and use them instead. See
        //   `initialize_from_current` for more details.
        // - Those that are not going to be used. For example, SS, DS, ES, FS, and GS
        //   are not initialized because x64 UEFI environment does not use them.
        self.host_gdt.initialize_from_current();
        let mut idtr = DescriptorTablePointer::<u64>::default();
        sidt(&mut idtr);
        vmwrite(vmcs::host::CS_SELECTOR, self.host_gdt.cs.bits());
        vmwrite(vmcs::host::TR_SELECTOR, self.host_gdt.tr.bits());
        vmwrite(vmcs::host::CR0, cr0().bits() as u64);
        vmwrite(vmcs::host::CR3, cr3());
        vmwrite(vmcs::host::CR4, cr4().bits() as u64);
        vmwrite(vmcs::host::TR_BASE, self.host_gdt.tss.0.as_ptr() as u64);
        vmwrite(vmcs::host::GDTR_BASE, self.host_gdt.gdtr.base as u64);
        vmwrite(vmcs::host::IDTR_BASE, idtr.base as u64);

        // Control Field. We configure as follows:
        // - Specify that the host should run in the long-mode.
        // - Specify that the guest should run in the long-mode.
        // - Enable VMX-preemption timer.
        // - Enable extended page tables.
        // - Intercept #BP, #UD, #PF as they can be indicator of bugs found by fuzzing.

        vmwrite(
            vmcs::control::VMEXIT_CONTROLS,
            adjust_vmx_control(VmxControl::VmExit, IA32_VMX_EXIT_CTLS_HOST_ADDRESS_SPACE_SIZE_FLAG),
        );

        vmwrite(
            vmcs::control::VMENTRY_CONTROLS,
            adjust_vmx_control(VmxControl::VmEntry, IA32_VMX_ENTRY_CTLS_IA32E_MODE_GUEST_FLAG),
        );

        // Enable VMX-preemption timer if available. We enable this feature to
        // gain control even if the guest is in an infinite loop.
        // See: 26.5.1 VMX-Preemption Timer
        vmwrite(
            vmcs::control::PINBASED_EXEC_CONTROLS,
            adjust_vmx_control(
                VmxControl::PinBased,
                IA32_VMX_PINBASED_CTLS_ACTIVATE_VMX_PREEMPTION_TIMER_FLAG,
            ),
        );

        vmwrite(
            vmcs::control::PRIMARY_PROCBASED_EXEC_CONTROLS,
            adjust_vmx_control(
                VmxControl::ProcessorBased,
                IA32_VMX_PROCBASED_CTLS_ACTIVATE_SECONDARY_CONTROLS_FLAG,
            ),
        );

        // Enable EPTs. This is a two-steps process at minimum:
        // - Set bit[1] of the secondary processor-based VM-execution controls.
        // See: Table 25-7. Definitions of Secondary Processor-Based VM-Execution
        //      Controls
        //
        // - Set the EPT pointer VMCS to point the EPT PML4.
        // The EPT pointer (EPTP) is a 64-bit VMCS field to tell the processor
        // the address of the EPT PML4. It is equivalent to the CR3 in the normal
        // paging structure walk, in a sense that it points to the base address
        // of the top level structure (PML4) to walk.
        // See: 25.6.11 Extended-Page-Table Pointer (EPTP)
        //
        // Lower 12-bits of EPTP is made up of flags.
        // - `EPT_POINTER_PAGE_WALK_LENGTH_4` specifies the maximum levels of EPTs,
        //   which is 4.
        // - `EPT_POINTER_MEMORY_TYPE_WRITE_BACK` specifies the write-back memory type
        //   for accessing to any of EPT paging-structures. This is most efficient.
        // See: 29.2.2 EPT Translation Mechanism
        // See: 29.2.6.1 Memory Type Used for Accessing EPT Paging Structures
        vmwrite(
            vmcs::control::SECONDARY_PROCBASED_EXEC_CONTROLS,
            adjust_vmx_control(
                VmxControl::ProcessorBased2,
                IA32_VMX_PROCBASED_CTLS2_ENABLE_EPT_FLAG,
            ),
        );
        vmwrite(
            vmcs::control::EPTP_FULL,
            nested_pml4_addr | EPT_POINTER_PAGE_WALK_LENGTH_4 | EPT_POINTER_MEMORY_TYPE_WRITE_BACK,
        );

        // Intercept #BP, #UD, #PF.
        // See: 25.6.3 Exception Bitmap
        vmwrite(
            vmcs::control::EXCEPTION_BITMAP,
            (1u64 << irq::BREAKPOINT_VECTOR)
                | (1u64 << irq::INVALID_OPCODE_VECTOR)
                | (1u64 << irq::PAGE_FAULT_VECTOR),
        );
    }

    /// Configures the guest states based on the snapshot.
    fn revert_registers(&mut self, snapshot: &Snapshot) {
        let registers = &snapshot.registers;

        // Guest-State Fields. We configure the guest based on the snapshot.
        // Some fields that are known to be zero are not explicitly set. For
        // example, the segment base registers.
        let guest_gdt_pfn = registers.gdtr.base as usize >> BASE_PAGE_SHIFT;
        let guest_gdt = addr_of!(snapshot.memory[guest_gdt_pfn]) as u64;
        vmwrite(vmcs::guest::ES_SELECTOR, registers.es);
        vmwrite(vmcs::guest::CS_SELECTOR, registers.cs);
        vmwrite(vmcs::guest::SS_SELECTOR, registers.ss);
        vmwrite(vmcs::guest::DS_SELECTOR, registers.ds);
        vmwrite(vmcs::guest::FS_SELECTOR, registers.fs);
        vmwrite(vmcs::guest::GS_SELECTOR, registers.gs);
        vmwrite(vmcs::guest::TR_SELECTOR, registers.tr);
        vmwrite(vmcs::guest::LDTR_SELECTOR, registers.ldtr);
        vmwrite(vmcs::guest::ES_ACCESS_RIGHTS, get_segment_access_right(guest_gdt, registers.es));
        vmwrite(vmcs::guest::CS_ACCESS_RIGHTS, get_segment_access_right(guest_gdt, registers.cs));
        vmwrite(vmcs::guest::SS_ACCESS_RIGHTS, get_segment_access_right(guest_gdt, registers.ss));
        vmwrite(vmcs::guest::DS_ACCESS_RIGHTS, get_segment_access_right(guest_gdt, registers.ds));
        vmwrite(vmcs::guest::FS_ACCESS_RIGHTS, get_segment_access_right(guest_gdt, registers.fs));
        vmwrite(vmcs::guest::GS_ACCESS_RIGHTS, get_segment_access_right(guest_gdt, registers.gs));
        vmwrite(vmcs::guest::TR_ACCESS_RIGHTS, get_segment_access_right(guest_gdt, registers.tr));
        vmwrite(
            vmcs::guest::LDTR_ACCESS_RIGHTS,
            get_segment_access_right(guest_gdt, registers.ldtr),
        );
        vmwrite(vmcs::guest::ES_LIMIT, get_segment_limit(guest_gdt, registers.es));
        vmwrite(vmcs::guest::CS_LIMIT, get_segment_limit(guest_gdt, registers.cs));
        vmwrite(vmcs::guest::SS_LIMIT, get_segment_limit(guest_gdt, registers.ss));
        vmwrite(vmcs::guest::DS_LIMIT, get_segment_limit(guest_gdt, registers.ds));
        vmwrite(vmcs::guest::FS_LIMIT, get_segment_limit(guest_gdt, registers.fs));
        vmwrite(vmcs::guest::GS_LIMIT, get_segment_limit(guest_gdt, registers.gs));
        vmwrite(vmcs::guest::TR_LIMIT, get_segment_limit(guest_gdt, registers.tr));
        vmwrite(vmcs::guest::LDTR_LIMIT, get_segment_limit(guest_gdt, registers.ldtr));
        vmwrite(vmcs::guest::FS_BASE, registers.fs_base);
        vmwrite(vmcs::guest::GS_BASE, registers.gs_base);
        vmwrite(vmcs::guest::TR_BASE, registers.tr_base);
        vmwrite(vmcs::guest::LDTR_BASE, registers.ldtr_base);
        vmwrite(vmcs::guest::GDTR_BASE, registers.gdtr.base as u64);
        vmwrite(vmcs::guest::GDTR_LIMIT, registers.gdtr.limit);
        vmwrite(vmcs::guest::IDTR_BASE, registers.idtr.base as u64);
        vmwrite(vmcs::guest::IDTR_LIMIT, registers.idtr.limit);
        vmwrite(vmcs::guest::IA32_SYSENTER_CS, registers.sysenter_cs);
        vmwrite(vmcs::guest::IA32_SYSENTER_ESP, registers.sysenter_esp);
        vmwrite(vmcs::guest::IA32_SYSENTER_EIP, registers.sysenter_eip);
        vmwrite(vmcs::guest::IA32_EFER_FULL, registers.efer);
        vmwrite(vmcs::guest::CR0, registers.cr0);
        vmwrite(vmcs::guest::CR3, registers.cr3);
        vmwrite(vmcs::guest::CR4, registers.cr4);
        vmwrite(vmcs::guest::RIP, registers.rip);
        vmwrite(vmcs::guest::RSP, registers.rsp);
        vmwrite(vmcs::guest::RFLAGS, registers.rflags);
        vmwrite(vmcs::guest::LINK_PTR_FULL, u64::MAX);

        // Set VMX-preemption timer counter if the processor supports it. Convert
        // TSC to the equivalent VMX-preemption timer count. The processor counts
        // this value down during the guest-mode and causes VM-exit when it becomes
        // zero.
        // See: 26.5.1 VMX-Preemption Timer
        if let Some(timer_scale) = self.timer_scale {
            vmwrite(
                vmcs::guest::VMX_PREEMPTION_TIMER_VALUE,
                GUEST_EXEC_TIMEOUT_IN_TSC / timer_scale,
            );
        };

        // Some registers are not managed by VMCS and needed to be manually saved
        // and loaded by software. General purpose registers are such examples.
        self.registers.rax = registers.rax;
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

    /// Executes the guest until it triggers VM-exit.
    fn run(&mut self) -> VmExitReason {
        const VMX_EXIT_REASON_EXCEPTION_OR_NMI: u16 = 0;
        const VMX_EXIT_REASON_TRIPLE_FAULT: u16 = 2;
        const VMX_EXIT_REASON_EPT_VIOLATION: u16 = 48;
        const VMX_EXIT_REASON_VMX_PREEMPTION_TIMER: u16 = 52;

        // Run the VM until the VM-exit occurs.
        let flags = unsafe { run_vm_vmx(&mut self.registers, u64::from(self.launched)) };
        vm_succeed(RFlags::from_raw(flags)).unwrap();
        self.launched = true;

        // VM-exit occurred. Copy the guest register values from VMCS so that
        // `self.registers` is complete and up to date.
        self.registers.rip = vmread(vmcs::guest::RIP);
        self.registers.rsp = vmread(vmcs::guest::RSP);
        self.registers.rflags = vmread(vmcs::guest::RFLAGS);

        // Handle VM-exit by translating it to the `VmExitReason` type.
        //
        // "VM exits begin by recording information about the nature of and reason
        //  for the VM exit in the VM-exit information fields."
        // See: 28.2 RECORDING VM-EXIT INFORMATION AND UPDATING VM-ENTRY CONTROL FIELDS
        //      28.2.1 Basic VM-Exit Information
        //
        // For the list of possible exit codes,
        // See: Table C-1. Basic Exit Reasons
        match vmread(vmcs::ro::EXIT_REASON) as u16 {
            // See: 26.2 OTHER CAUSES OF VM EXITS
            //      25.9.2 Information for VM Exits Due to Vectored Events
            VMX_EXIT_REASON_EXCEPTION_OR_NMI => VmExitReason::Exception(ExceptionQualification {
                rip: self.registers.rip,
                exception_code: GuestException::try_from(
                    vmread(vmcs::ro::VMEXIT_INTERRUPTION_INFO) as u8,
                )
                .unwrap(),
            }),
            // See: 29.3.3.2 EPT Violations
            //      28.2.1 Basic VM-Exit Information
            //      Table 28-7. Exit Qualification for EPT Violations
            VMX_EXIT_REASON_EPT_VIOLATION => {
                let qualification = vmread(vmcs::ro::EXIT_QUALIFICATION);
                VmExitReason::NestedPageFault(NestedPageFaultQualification {
                    rip: self.registers.rip,
                    gpa: vmread(vmcs::ro::GUEST_PHYSICAL_ADDR_FULL),
                    missing_translation: (qualification & 0b11_1000) == 0,
                    write_access: (qualification & 0b10) != 0,
                })
            }
            // See: 26.5.1 VMX-Preemption Timer
            VMX_EXIT_REASON_VMX_PREEMPTION_TIMER => VmExitReason::TimerExpiration,
            // See: 26.2 OTHER CAUSES OF VM EXITS
            VMX_EXIT_REASON_TRIPLE_FAULT => VmExitReason::Shutdown(vmread(vmcs::ro::EXIT_REASON)),
            // Anything else.
            _ => VmExitReason::Unexpected(vmread(vmcs::ro::EXIT_REASON)),
        }
    }

    /// Invalidates caches of the extended page tables.
    fn invalidate_caches(&mut self) {
        // Note that this is NOT required unless we enable VPID, which we do not.
        // When VPID is not enabled, caches are always invalidated on VM-exit and
        // VM-entry. The code is left as a reference and for clarity.
        // See: 29.4.3.1 Operations that Invalidate Cached Mappings
        invept(InveptType::SingleContext, vmread(vmcs::control::EPTP_FULL));
    }

    /// Gets a flag value to be set to nested paging structure entries for the
    /// given entry types (eg, permissions).
    fn nps_entry_flags(
        &self,
        entry_type: NestedPagingStructureEntryType,
    ) -> NestedPagingStructureEntryFlags {
        // See: Table 29-6. Format of an EPT Page-Table Entry that Maps a 4-KByte Page
        match entry_type {
            // RWX
            NestedPagingStructureEntryType::Rwx => NestedPagingStructureEntryFlags {
                permission: 0b111,
                memory_type: 0,
            },
            // RWX | WB
            NestedPagingStructureEntryType::RwxWriteBack => NestedPagingStructureEntryFlags {
                permission: 0b111,
                memory_type: 6,
            },
            // R-X | WB
            NestedPagingStructureEntryType::RxWriteBack => NestedPagingStructureEntryFlags {
                permission: 0b101,
                memory_type: 6,
            },
        }
    }
}

const IA32_VMX_PINBASED_CTLS_ACTIVATE_VMX_PREEMPTION_TIMER_FLAG: u64 = 1 << 6;

impl Vmx {
    pub(crate) fn new() -> Self {
        /// Returns the scale value to convert TSC to the unit where
        /// VMX-preemption timer VMCS expects.
        fn vmx_preemption_timer_scale() -> Option<u64> {
            if (adjust_vmx_control(
                VmxControl::PinBased,
                IA32_VMX_PINBASED_CTLS_ACTIVATE_VMX_PREEMPTION_TIMER_FLAG,
            ) & IA32_VMX_PINBASED_CTLS_ACTIVATE_VMX_PREEMPTION_TIMER_FLAG)
                == 0
            {
                warn!("VMX-preemption timer not available. Dead loop is possible!");
                None
            } else {
                const IA32_VMX_MISC_PREEMPTION_TIMER_TSC_RELATIONSHIP_MASK: u64 = 0b11111;

                let bit_position = rdmsr(x86::msr::IA32_VMX_MISC)
                    & IA32_VMX_MISC_PREEMPTION_TIMER_TSC_RELATIONSHIP_MASK;
                let vmx_timer_scale = 1 << bit_position;
                debug!("VMX-preemption timer scale {vmx_timer_scale}");
                Some(vmx_timer_scale)
            }
        }

        let vmxon_region = unsafe { Box::<Vmxon>::new_zeroed().assume_init() };
        let vmcs_region = unsafe { Box::<Vmcs>::new_zeroed().assume_init() };
        Self {
            vmxon_region,
            vmcs_region,
            timer_scale: vmx_preemption_timer_scale(),
            ..Default::default()
        }
    }
}

/// The region of memory that the logical processor uses to support VMX
/// operation.
///
/// See: 25.11.5 VMXON Region
#[derive(derivative::Derivative)]
#[derivative(Default)]
#[repr(C, align(4096))]
struct Vmxon {
    revision_id: u32,
    #[derivative(Default(value = "[0; 4092]"))]
    data: [u8; 4092],
}
const _: () = assert!(size_of::<Vmxon>() == 0x1000);

/// The region of memory that the logical processor uses to represent a virtual
/// CPU. Called virtual-machine control data structure (VMCS).
///
/// See: 25.2 FORMAT OF THE VMCS REGION
#[derive(derivative::Derivative)]
#[derivative(Default)]
#[repr(C, align(4096))]
struct Vmcs {
    revision_id: u32,
    abort_indicator: u32,
    #[derivative(Default(value = "[0; 4088]"))]
    data: [u8; 4088],
}
const _: () = assert!(size_of::<Vmcs>() == 0x1000);

/// The types of the control field.
#[derive(Clone, Copy)]
enum VmxControl {
    PinBased,
    ProcessorBased,
    ProcessorBased2,
    VmExit,
    VmEntry,
}

/// The type of invalidation the INVEPT instruction performs.
///
/// See: 29.4.3.1 Operations that Invalidate Cached Mappings
#[repr(u64)]
enum InveptType {
    SingleContext = 1,
}

/// The structure to specify the effect of the INVEPT instruction.
///
/// See: Figure 31-1. INVEPT Descriptor
#[repr(C)]
struct InveptDescriptor {
    eptp: u64,
    _reserved: u64,
}
const _: () = assert!(size_of::<InveptDescriptor>() == 16);

/// The collection of GDT related data needed to manage the host GDT.
#[repr(C, align(16))]
struct HostGdt {
    gdt: Vec<u64>,
    gdtr: DescriptorTablePointer<u64>,
    tss: TaskStateSegment,
    tr: SegmentSelector,
    cs: SegmentSelector,
}
const _: () = assert!((size_of::<HostGdt>() % 0x10) == 0);

impl HostGdt {
    /// Initializes the host GDT from the current GDT.
    ///
    /// This function exists because, on the UEFI DXE phase, the Task Register
    /// (TR) is zero which does not satisfy requirements as host state. To
    /// workaround this, this function makes a clone of the current GDT,
    /// adds TSS, and initializes TR and GDTR with the clone to be used for as
    /// host state.
    ///
    /// "The selector fields for CS and TR cannot be 0000H."
    /// See: 27.2.3 Checks on Host Segment and Descriptor-Table Registers
    fn initialize_from_current(&mut self) {
        // Clone the current GDT first.
        let mut current_gdtr = DescriptorTablePointer::<u64>::default();
        sgdt(&mut current_gdtr);
        let current_gdt = unsafe {
            core::slice::from_raw_parts(
                current_gdtr.base.cast::<u64>(),
                usize::from(current_gdtr.limit + 1) / 8,
            )
        };
        self.gdt = current_gdt.to_vec();

        // Then, append one more entry for the task state segment.
        self.gdt.push(task_segment_descriptor(&self.tss));

        // Fill in the GDTR according to the new GDT.
        self.gdtr.base = self.gdt.as_ptr();
        self.gdtr.limit = u16::try_from(self.gdt.len() * 8 - 1).unwrap();

        // Finally, compute an index (TR) that point to the last entry in the GDT.
        let tr_index = self.gdt.len() as u16 - 1;
        self.tr = SegmentSelector::new(tr_index, x86::Ring::Ring0);
        self.cs = x86::segmentation::cs();
    }
}

impl Default for HostGdt {
    fn default() -> Self {
        Self {
            gdt: Vec::new(),
            gdtr: DescriptorTablePointer::<u64>::default(),
            tss: TaskStateSegment([0; 104]),
            tr: SegmentSelector::from_raw(0),
            cs: SegmentSelector::from_raw(0),
        }
    }
}

/// See: Figure 8-11. 64-Bit TSS Format
struct TaskStateSegment([u8; 104]);

/// Builds a segment descriptor from the task state segment.
fn task_segment_descriptor(tss: &TaskStateSegment) -> u64 {
    let tss_size = size_of::<TaskStateSegment>() as u64;
    let tss_base = core::ptr::from_ref::<TaskStateSegment>(tss) as u64;
    let tss_descriptor = <DescriptorBuilder as GateDescriptorBuilder<u32>>::tss_descriptor(
        tss_base,
        tss_size - 1,
        true,
    )
    .present()
    .dpl(x86::Ring::Ring0)
    .finish();
    unsafe { core::mem::transmute::<Descriptor, u64>(tss_descriptor) }
}

/// Returns an adjust value for the control field according to the capability
/// MSR.
fn adjust_vmx_control(control: VmxControl, requested_value: u64) -> u64 {
    const IA32_VMX_BASIC_VMX_CONTROLS_FLAG: u64 = 1 << 55;

    // This determines the right VMX capability MSR based on the value of
    // IA32_VMX_BASIC. This is required to fullfil the following requirements:
    //
    // "It is necessary for software to consult only one of the capability MSRs
    //  to determine the allowed settings of the pin based VM-execution controls:"
    // See: A.3.1 Pin-Based VM-Execution Controls
    let vmx_basic = rdmsr(x86::msr::IA32_VMX_BASIC);
    let true_cap_msr_supported = (vmx_basic & IA32_VMX_BASIC_VMX_CONTROLS_FLAG) != 0;

    let cap_msr = match (control, true_cap_msr_supported) {
        (VmxControl::PinBased, true) => x86::msr::IA32_VMX_TRUE_PINBASED_CTLS,
        (VmxControl::PinBased, false) => x86::msr::IA32_VMX_PINBASED_CTLS,
        (VmxControl::ProcessorBased, true) => x86::msr::IA32_VMX_TRUE_PROCBASED_CTLS,
        (VmxControl::ProcessorBased, false) => x86::msr::IA32_VMX_PROCBASED_CTLS,
        (VmxControl::VmExit, true) => x86::msr::IA32_VMX_TRUE_EXIT_CTLS,
        (VmxControl::VmExit, false) => x86::msr::IA32_VMX_EXIT_CTLS,
        (VmxControl::VmEntry, true) => x86::msr::IA32_VMX_TRUE_ENTRY_CTLS,
        (VmxControl::VmEntry, false) => x86::msr::IA32_VMX_ENTRY_CTLS,
        // There is no TRUE MSR for IA32_VMX_PROCBASED_CTLS2. Just use
        // IA32_VMX_PROCBASED_CTLS2 unconditionally.
        (VmxControl::ProcessorBased2, _) => x86::msr::IA32_VMX_PROCBASED_CTLS2,
    };

    // Each bit of the following VMCS values might have to be set or cleared
    // according to the value indicated by the VMX capability MSRs.
    //  - pin-based VM-execution controls,
    //  - primary processor-based VM-execution controls,
    //  - secondary processor-based VM-execution controls.
    //
    // The VMX capability MSR is composed of two 32bit values, the lower 32bits
    // indicate bits can be 0, and the higher 32bits indicates bits can be 1.
    // In other words, if those bits are "cleared", corresponding bits MUST BE 1
    // and MUST BE 0 respectively. The below summarizes the interpretation:
    //
    //        Lower bits (allowed 0) Higher bits (allowed 1) Meaning
    // Bit X  1                      1                       The bit X is flexible
    // Bit X  1                      0                       The bit X is fixed to 0
    // Bit X  0                      1                       The bit X is fixed to 1
    //
    // The following code enforces this logic by setting bits that must be 1,
    // and clearing bits that must be 0.
    //
    // See: A.3.1 Pin-Based VM-Execution Controls
    // See: A.3.2 Primary Processor-Based VM-Execution Controls
    // See: A.3.3 Secondary Processor-Based VM-Execution Controls
    let capabilities = rdmsr(cap_msr);
    let allowed0 = capabilities as u32;
    let allowed1 = (capabilities >> 32) as u32;
    let mut effective_value = u32::try_from(requested_value).unwrap();
    effective_value |= allowed0;
    effective_value &= allowed1;
    u64::from(effective_value)
}

/// Updates the `IA32_FEATURE_CONTROL` MSR to satisfy the requirement for
/// entering VMX operation.
fn adjust_feature_control_msr() {
    const IA32_FEATURE_CONTROL_LOCK_BIT_FLAG: u64 = 1 << 0;
    const IA32_FEATURE_CONTROL_ENABLE_VMX_OUTSIDE_SMX_FLAG: u64 = 1 << 2;

    // If the lock bit is cleared, set it along with the VMXON-outside-SMX
    // operation bit. Without those two bits, the VMXON instruction fails. They
    // are normally set but not always, for example, Bochs with OVFM does not.
    // See: 23.7 ENABLING AND ENTERING VMX OPERATION
    let feature_control = rdmsr(x86::msr::IA32_FEATURE_CONTROL);
    if (feature_control & IA32_FEATURE_CONTROL_LOCK_BIT_FLAG) == 0 {
        wrmsr(
            x86::msr::IA32_FEATURE_CONTROL,
            feature_control
                | IA32_FEATURE_CONTROL_ENABLE_VMX_OUTSIDE_SMX_FLAG
                | IA32_FEATURE_CONTROL_LOCK_BIT_FLAG,
        );
    }
}

/// Updates the CR0 to satisfy the requirement for entering VMX
/// operation.
fn adjust_cr0() {
    // In order to enter VMX operation, some bits in CR0 (and CR4) have to be
    // set or cleared as indicated by the FIXED0 and FIXED1 MSRs. The rule is
    // summarized as below (taking CR0 as an example):
    //
    //        IA32_VMX_CR0_FIXED0 IA32_VMX_CR0_FIXED1 Meaning
    // Bit X  1                   (Always 1)          The bit X of CR0 is fixed to 1
    // Bit X  0                   1                   The bit X of CR0 is flexible
    // Bit X  (Always 0)          0                   The bit X of CR0 is fixed to 0
    //
    // Some UEFI implementations do not fullfil those requirements for CR0 and
    // need adjustments. The requirements for CR4 are always satisfied as far
    // as the author has experimented (although not guaranteed).
    //
    // See: A.7 VMX-FIXED BITS IN CR0
    // See: A.8 VMX-FIXED BITS IN CR4
    let fixed0cr0 = rdmsr(x86::msr::IA32_VMX_CR0_FIXED0);
    let fixed1cr0 = rdmsr(x86::msr::IA32_VMX_CR0_FIXED1);
    let mut new_cr0 = cr0().bits() as u64;
    new_cr0 &= fixed1cr0;
    new_cr0 |= fixed0cr0;
    let new_cr0 = Cr0::from_bits_truncate(new_cr0 as usize);
    cr0_write(new_cr0);
}

/// Returns the access rights of the given segment for VMX.
fn get_segment_access_right(table_base: u64, selector: u16) -> u32 {
    const VMX_SEGMENT_ACCESS_RIGHTS_UNUSABLE_FLAG: u32 = 1 << 16;

    let sel = SegmentSelector::from_raw(selector);
    if sel.index() == 0 && (sel.bits() >> 2) == 0 {
        return VMX_SEGMENT_ACCESS_RIGHTS_UNUSABLE_FLAG;
    }
    let descriptor_value = get_segment_descriptor_value(table_base, selector);

    // Get the Type, S, DPL, P, AVL, L, D/B and G bits from the segment descriptor.
    // See: Figure 3-8. Segment Descriptor
    let ar = (descriptor_value >> 40) as u32;
    ar & 0b1111_0000_1111_1111
}

extern "efiapi" {
    /// Runs the guest until VM-exit occurs.
    fn run_vm_vmx(registers: &mut GuestRegisters, launched: u64) -> u64;
}
global_asm!(include_str!("vmx_run_vm.S"));

/// The wrapper of the VMXON instruction.
fn vmxon(vmxon_region: &mut Vmxon) {
    // Safety: this project runs at CPL0.
    unsafe { x86::bits64::vmx::vmxon(core::ptr::from_mut(vmxon_region) as u64).unwrap() };
}

/// The wrapper of the VMCLEAR instruction.
fn vmclear(vmcs_region: &mut Vmcs) {
    // Safety: this project runs at CPL0.
    unsafe { x86::bits64::vmx::vmclear(core::ptr::from_mut(vmcs_region) as u64).unwrap() };
}

/// The wrapper of the VMPTRLD instruction.
fn vmptrld(vmcs_region: &mut Vmcs) {
    // Safety: this project runs at CPL0.
    unsafe { x86::bits64::vmx::vmptrld(core::ptr::from_mut(vmcs_region) as u64).unwrap() }
}

/// The wrapper of the VMPTRST instruction.
fn vmptrst() -> *const Vmcs {
    // Safety: this project runs at CPL0.
    unsafe { x86::bits64::vmx::vmptrst().unwrap() as *const Vmcs }
}

/// The wrapper of the VMREAD instruction. Returns zero on error.
fn vmread(field: u32) -> u64 {
    // Safety: this project runs at CPL0.
    unsafe { x86::bits64::vmx::vmread(field) }.unwrap_or(0)
}

/// The wrapper of the VMWRITE instruction.
fn vmwrite<T: Into<u64>>(field: u32, val: T)
where
    u64: From<T>,
{
    // Safety: this project runs at CPL0.
    unsafe { x86::bits64::vmx::vmwrite(field, u64::from(val)) }.unwrap();
}

/// The wrapper of the INVEPT instruction.
///
/// See: INVEPT - Invalidate Translations Derived from EPT
fn invept(invalidation: InveptType, eptp: u64) {
    let descriptor = InveptDescriptor { eptp, _reserved: 0 };
    let flags = unsafe {
        let flags: u64;
        asm!(
            "invept {}, [{}]",
            "pushfq",
            "pop {}",
            in(reg) invalidation as u64,
            in(reg) &descriptor,
            lateout(reg) flags
        );
        flags
    };
    if let Err(err) = vm_succeed(RFlags::from_raw(flags)) {
        panic!("{err}");
    }
}

/// Checks that the latest VMX instruction succeeded.
///
/// See: 31.2 CONVENTIONS
fn vm_succeed(flags: RFlags) -> Result<(), String> {
    if flags.contains(RFlags::FLAGS_ZF) {
        // See: 31.4 VM INSTRUCTION ERROR NUMBERS
        Err(format!("VmFailValid with {}", vmread(vmcs::ro::VM_INSTRUCTION_ERROR)))
    } else if flags.contains(RFlags::FLAGS_CF) {
        Err("VmFailInvalid".to_string())
    } else {
        Ok(())
    }
}

impl fmt::Debug for Vmcs {
    #[rustfmt::skip]
    #[allow(clippy::too_many_lines)]
    fn fmt(&self, format: &mut fmt::Formatter<'_>) -> fmt::Result {
        assert!(core::ptr::from_ref(self) == vmptrst());

        // Dump the current VMCS. Not that this is not exhaustive.
        format.debug_struct("Vmcs")
        .field("Current VMCS", &core::ptr::from_ref(self))
        .field("Revision ID", &self.revision_id)

        // 16-Bit Guest-State Fields
        .field("Guest ES Selector                              ", &vmread(vmcs::guest::ES_SELECTOR))
        .field("Guest CS Selector                              ", &vmread(vmcs::guest::CS_SELECTOR))
        .field("Guest SS Selector                              ", &vmread(vmcs::guest::SS_SELECTOR))
        .field("Guest DS Selector                              ", &vmread(vmcs::guest::DS_SELECTOR))
        .field("Guest FS Selector                              ", &vmread(vmcs::guest::FS_SELECTOR))
        .field("Guest GS Selector                              ", &vmread(vmcs::guest::GS_SELECTOR))
        .field("Guest LDTR Selector                            ", &vmread(vmcs::guest::LDTR_SELECTOR))
        .field("Guest TR Selector                              ", &vmread(vmcs::guest::TR_SELECTOR))
        .field("Guest interrupt status                         ", &vmread(vmcs::guest::INTERRUPT_STATUS))
        .field("PML index                                      ", &vmread(vmcs::guest::PML_INDEX))

        // 64-Bit Guest-State Fields
        .field("VMCS link pointer                              ", &vmread(vmcs::guest::LINK_PTR_FULL))
        .field("Guest IA32_DEBUGCTL                            ", &vmread(vmcs::guest::IA32_DEBUGCTL_FULL))
        .field("Guest IA32_PAT                                 ", &vmread(vmcs::guest::IA32_PAT_FULL))
        .field("Guest IA32_EFER                                ", &vmread(vmcs::guest::IA32_EFER_FULL))
        .field("Guest IA32_PERF_GLOBAL_CTRL                    ", &vmread(vmcs::guest::IA32_PERF_GLOBAL_CTRL_FULL))
        .field("Guest PDPTE0                                   ", &vmread(vmcs::guest::PDPTE0_FULL))
        .field("Guest PDPTE1                                   ", &vmread(vmcs::guest::PDPTE1_FULL))
        .field("Guest PDPTE2                                   ", &vmread(vmcs::guest::PDPTE2_FULL))
        .field("Guest PDPTE3                                   ", &vmread(vmcs::guest::PDPTE3_FULL))
        .field("Guest IA32_BNDCFGS                             ", &vmread(vmcs::guest::IA32_BNDCFGS_FULL))
        .field("Guest IA32_RTIT_CTL                            ", &vmread(vmcs::guest::IA32_RTIT_CTL_FULL))

        // 32-Bit Guest-State Fields
        .field("Guest ES Limit                                 ", &vmread(vmcs::guest::ES_LIMIT))
        .field("Guest CS Limit                                 ", &vmread(vmcs::guest::CS_LIMIT))
        .field("Guest SS Limit                                 ", &vmread(vmcs::guest::SS_LIMIT))
        .field("Guest DS Limit                                 ", &vmread(vmcs::guest::DS_LIMIT))
        .field("Guest FS Limit                                 ", &vmread(vmcs::guest::FS_LIMIT))
        .field("Guest GS Limit                                 ", &vmread(vmcs::guest::GS_LIMIT))
        .field("Guest LDTR Limit                               ", &vmread(vmcs::guest::LDTR_LIMIT))
        .field("Guest TR Limit                                 ", &vmread(vmcs::guest::TR_LIMIT))
        .field("Guest GDTR limit                               ", &vmread(vmcs::guest::GDTR_LIMIT))
        .field("Guest IDTR limit                               ", &vmread(vmcs::guest::IDTR_LIMIT))
        .field("Guest ES access rights                         ", &vmread(vmcs::guest::ES_ACCESS_RIGHTS))
        .field("Guest CS access rights                         ", &vmread(vmcs::guest::CS_ACCESS_RIGHTS))
        .field("Guest SS access rights                         ", &vmread(vmcs::guest::SS_ACCESS_RIGHTS))
        .field("Guest DS access rights                         ", &vmread(vmcs::guest::DS_ACCESS_RIGHTS))
        .field("Guest FS access rights                         ", &vmread(vmcs::guest::FS_ACCESS_RIGHTS))
        .field("Guest GS access rights                         ", &vmread(vmcs::guest::GS_ACCESS_RIGHTS))
        .field("Guest LDTR access rights                       ", &vmread(vmcs::guest::LDTR_ACCESS_RIGHTS))
        .field("Guest TR access rights                         ", &vmread(vmcs::guest::TR_ACCESS_RIGHTS))
        .field("Guest interruptibility state                   ", &vmread(vmcs::guest::INTERRUPTIBILITY_STATE))
        .field("Guest activity state                           ", &vmread(vmcs::guest::ACTIVITY_STATE))
        .field("Guest SMBASE                                   ", &vmread(vmcs::guest::SMBASE))
        .field("Guest IA32_SYSENTER_CS                         ", &vmread(vmcs::guest::IA32_SYSENTER_CS))
        .field("VMX-preemption timer value                     ", &vmread(vmcs::guest::VMX_PREEMPTION_TIMER_VALUE))

        // Natural-Width Guest-State Fields
        .field("Guest CR0                                      ", &vmread(vmcs::guest::CR0))
        .field("Guest CR3                                      ", &vmread(vmcs::guest::CR3))
        .field("Guest CR4                                      ", &vmread(vmcs::guest::CR4))
        .field("Guest ES Base                                  ", &vmread(vmcs::guest::ES_BASE))
        .field("Guest CS Base                                  ", &vmread(vmcs::guest::CS_BASE))
        .field("Guest SS Base                                  ", &vmread(vmcs::guest::SS_BASE))
        .field("Guest DS Base                                  ", &vmread(vmcs::guest::DS_BASE))
        .field("Guest FS Base                                  ", &vmread(vmcs::guest::FS_BASE))
        .field("Guest GS Base                                  ", &vmread(vmcs::guest::GS_BASE))
        .field("Guest LDTR base                                ", &vmread(vmcs::guest::LDTR_BASE))
        .field("Guest TR base                                  ", &vmread(vmcs::guest::TR_BASE))
        .field("Guest GDTR base                                ", &vmread(vmcs::guest::GDTR_BASE))
        .field("Guest IDTR base                                ", &vmread(vmcs::guest::IDTR_BASE))
        .field("Guest DR7                                      ", &vmread(vmcs::guest::DR7))
        .field("Guest RSP                                      ", &vmread(vmcs::guest::RSP))
        .field("Guest RIP                                      ", &vmread(vmcs::guest::RIP))
        .field("Guest RFLAGS                                   ", &vmread(vmcs::guest::RFLAGS))
        .field("Guest pending debug exceptions                 ", &vmread(vmcs::guest::PENDING_DBG_EXCEPTIONS))
        .field("Guest IA32_SYSENTER_ESP                        ", &vmread(vmcs::guest::IA32_SYSENTER_ESP))
        .field("Guest IA32_SYSENTER_EIP                        ", &vmread(vmcs::guest::IA32_SYSENTER_EIP))

        // 16-Bit Host-State Fields
        .field("Host ES Selector                               ", &vmread(vmcs::host::ES_SELECTOR))
        .field("Host CS Selector                               ", &vmread(vmcs::host::CS_SELECTOR))
        .field("Host SS Selector                               ", &vmread(vmcs::host::SS_SELECTOR))
        .field("Host DS Selector                               ", &vmread(vmcs::host::DS_SELECTOR))
        .field("Host FS Selector                               ", &vmread(vmcs::host::FS_SELECTOR))
        .field("Host GS Selector                               ", &vmread(vmcs::host::GS_SELECTOR))
        .field("Host TR Selector                               ", &vmread(vmcs::host::TR_SELECTOR))

        // 64-Bit Host-State Fields
        .field("Host IA32_PAT                                  ", &vmread(vmcs::host::IA32_PAT_FULL))
        .field("Host IA32_EFER                                 ", &vmread(vmcs::host::IA32_EFER_FULL))
        .field("Host IA32_PERF_GLOBAL_CTRL                     ", &vmread(vmcs::host::IA32_PERF_GLOBAL_CTRL_FULL))

        // 32-Bit Host-State Fields
        .field("Host IA32_SYSENTER_CS                          ", &vmread(vmcs::host::IA32_SYSENTER_CS))

        // Natural-Width Host-State Fields
        .field("Host CR0                                       ", &vmread(vmcs::host::CR0))
        .field("Host CR3                                       ", &vmread(vmcs::host::CR3))
        .field("Host CR4                                       ", &vmread(vmcs::host::CR4))
        .field("Host FS Base                                   ", &vmread(vmcs::host::FS_BASE))
        .field("Host GS Base                                   ", &vmread(vmcs::host::GS_BASE))
        .field("Host TR base                                   ", &vmread(vmcs::host::TR_BASE))
        .field("Host GDTR base                                 ", &vmread(vmcs::host::GDTR_BASE))
        .field("Host IDTR base                                 ", &vmread(vmcs::host::IDTR_BASE))
        .field("Host IA32_SYSENTER_ESP                         ", &vmread(vmcs::host::IA32_SYSENTER_ESP))
        .field("Host IA32_SYSENTER_EIP                         ", &vmread(vmcs::host::IA32_SYSENTER_EIP))
        .field("Host RSP                                       ", &vmread(vmcs::host::RSP))
        .field("Host RIP                                       ", &vmread(vmcs::host::RIP))

        // 16-Bit Control Fields
        .field("Virtual-processor identifier                   ", &vmread(vmcs::control::VPID))
        .field("Posted-interrupt notification vector           ", &vmread(vmcs::control::POSTED_INTERRUPT_NOTIFICATION_VECTOR))
        .field("EPTP index                                     ", &vmread(vmcs::control::EPTP_INDEX))

        // 64-Bit Control Fields
        .field("Address of I/O bitmap A                        ", &vmread(vmcs::control::IO_BITMAP_A_ADDR_FULL))
        .field("Address of I/O bitmap B                        ", &vmread(vmcs::control::IO_BITMAP_B_ADDR_FULL))
        .field("Address of MSR bitmaps                         ", &vmread(vmcs::control::MSR_BITMAPS_ADDR_FULL))
        .field("VM-exit MSR-store address                      ", &vmread(vmcs::control::VMEXIT_MSR_STORE_ADDR_FULL))
        .field("VM-exit MSR-load address                       ", &vmread(vmcs::control::VMEXIT_MSR_LOAD_ADDR_FULL))
        .field("VM-entry MSR-load address                      ", &vmread(vmcs::control::VMENTRY_MSR_LOAD_ADDR_FULL))
        .field("Executive-VMCS pointer                         ", &vmread(vmcs::control::EXECUTIVE_VMCS_PTR_FULL))
        .field("PML address                                    ", &vmread(vmcs::control::PML_ADDR_FULL))
        .field("TSC offset                                     ", &vmread(vmcs::control::TSC_OFFSET_FULL))
        .field("Virtual-APIC address                           ", &vmread(vmcs::control::VIRT_APIC_ADDR_FULL))
        .field("APIC-access address                            ", &vmread(vmcs::control::APIC_ACCESS_ADDR_FULL))
        .field("Posted-interrupt descriptor address            ", &vmread(vmcs::control::POSTED_INTERRUPT_DESC_ADDR_FULL))
        .field("VM-function controls                           ", &vmread(vmcs::control::VM_FUNCTION_CONTROLS_FULL))
        .field("EPT pointer                                    ", &vmread(vmcs::control::EPTP_FULL))
        .field("EOI-exit bitmap 0                              ", &vmread(vmcs::control::EOI_EXIT0_FULL))
        .field("EOI-exit bitmap 1                              ", &vmread(vmcs::control::EOI_EXIT1_FULL))
        .field("EOI-exit bitmap 2                              ", &vmread(vmcs::control::EOI_EXIT2_FULL))
        .field("EOI-exit bitmap 3                              ", &vmread(vmcs::control::EOI_EXIT3_FULL))
        .field("EPTP-list address                              ", &vmread(vmcs::control::EPTP_LIST_ADDR_FULL))
        .field("VMREAD-bitmap address                          ", &vmread(vmcs::control::VMREAD_BITMAP_ADDR_FULL))
        .field("VMWRITE-bitmap address                         ", &vmread(vmcs::control::VMWRITE_BITMAP_ADDR_FULL))
        .field("Virtualization-exception information address   ", &vmread(vmcs::control::VIRT_EXCEPTION_INFO_ADDR_FULL))
        .field("XSS-exiting bitmap                             ", &vmread(vmcs::control::XSS_EXITING_BITMAP_FULL))
        .field("ENCLS-exiting bitmap                           ", &vmread(vmcs::control::ENCLS_EXITING_BITMAP_FULL))
        .field("Sub-page-permission-table pointer              ", &vmread(vmcs::control::SUBPAGE_PERM_TABLE_PTR_FULL))
        .field("TSC multiplier                                 ", &vmread(vmcs::control::TSC_MULTIPLIER_FULL))

        // 32-Bit Control Fields
        .field("Pin-based VM-execution controls                ", &vmread(vmcs::control::PINBASED_EXEC_CONTROLS))
        .field("Primary processor-based VM-execution controls  ", &vmread(vmcs::control::PRIMARY_PROCBASED_EXEC_CONTROLS))
        .field("Exception bitmap                               ", &vmread(vmcs::control::EXCEPTION_BITMAP))
        .field("Page-fault error-code mask                     ", &vmread(vmcs::control::PAGE_FAULT_ERR_CODE_MASK))
        .field("Page-fault error-code match                    ", &vmread(vmcs::control::PAGE_FAULT_ERR_CODE_MATCH))
        .field("CR3-target count                               ", &vmread(vmcs::control::CR3_TARGET_COUNT))
        .field("Primary VM-exit controls                       ", &vmread(vmcs::control::VMEXIT_CONTROLS))
        .field("VM-exit MSR-store count                        ", &vmread(vmcs::control::VMEXIT_MSR_STORE_COUNT))
        .field("VM-exit MSR-load count                         ", &vmread(vmcs::control::VMEXIT_MSR_LOAD_COUNT))
        .field("VM-entry controls                              ", &vmread(vmcs::control::VMENTRY_CONTROLS))
        .field("VM-entry MSR-load count                        ", &vmread(vmcs::control::VMENTRY_MSR_LOAD_COUNT))
        .field("VM-entry interruption-information field        ", &vmread(vmcs::control::VMENTRY_INTERRUPTION_INFO_FIELD))
        .field("VM-entry exception error code                  ", &vmread(vmcs::control::VMENTRY_EXCEPTION_ERR_CODE))
        .field("VM-entry instruction length                    ", &vmread(vmcs::control::VMENTRY_INSTRUCTION_LEN))
        .field("TPR threshold                                  ", &vmread(vmcs::control::TPR_THRESHOLD))
        .field("Secondary processor-based VM-execution controls", &vmread(vmcs::control::SECONDARY_PROCBASED_EXEC_CONTROLS))
        .field("PLE_Gap                                        ", &vmread(vmcs::control::PLE_GAP))
        .field("PLE_Window                                     ", &vmread(vmcs::control::PLE_WINDOW))

        // Natural-Width Control Fields
        .field("CR0 guest/host mask                            ", &vmread(vmcs::control::CR0_GUEST_HOST_MASK))
        .field("CR4 guest/host mask                            ", &vmread(vmcs::control::CR4_GUEST_HOST_MASK))
        .field("CR0 read shadow                                ", &vmread(vmcs::control::CR0_READ_SHADOW))
        .field("CR4 read shadow                                ", &vmread(vmcs::control::CR4_READ_SHADOW))
        .field("CR3-target value 0                             ", &vmread(vmcs::control::CR3_TARGET_VALUE0))
        .field("CR3-target value 1                             ", &vmread(vmcs::control::CR3_TARGET_VALUE1))
        .field("CR3-target value 2                             ", &vmread(vmcs::control::CR3_TARGET_VALUE2))
        .field("CR3-target value 3                             ", &vmread(vmcs::control::CR3_TARGET_VALUE3))

        // 16-Bit Read-Only Data Fields

        // 64-Bit Read-Only Data Fields
        .field("Guest-physical address                         ", &vmread(vmcs::ro::GUEST_PHYSICAL_ADDR_FULL))

        // 32-Bit Read-Only Data Fields
        .field("VM-instruction error                           ", &vmread(vmcs::ro::VM_INSTRUCTION_ERROR))
        .field("Exit reason                                    ", &vmread(vmcs::ro::EXIT_REASON))
        .field("VM-exit interruption information               ", &vmread(vmcs::ro::VMEXIT_INTERRUPTION_INFO))
        .field("VM-exit interruption error code                ", &vmread(vmcs::ro::VMEXIT_INTERRUPTION_ERR_CODE))
        .field("IDT-vectoring information field                ", &vmread(vmcs::ro::IDT_VECTORING_INFO))
        .field("IDT-vectoring error code                       ", &vmread(vmcs::ro::IDT_VECTORING_ERR_CODE))
        .field("VM-exit instruction length                     ", &vmread(vmcs::ro::VMEXIT_INSTRUCTION_LEN))
        .field("VM-exit instruction information                ", &vmread(vmcs::ro::VMEXIT_INSTRUCTION_INFO))

        // Natural-Width Read-Only Data Fields
        .field("Exit qualification                             ", &vmread(vmcs::ro::EXIT_QUALIFICATION))
        .field("I/O RCX                                        ", &vmread(vmcs::ro::IO_RCX))
        .field("I/O RSI                                        ", &vmread(vmcs::ro::IO_RSI))
        .field("I/O RDI                                        ", &vmread(vmcs::ro::IO_RDI))
        .field("I/O RIP                                        ", &vmread(vmcs::ro::IO_RIP))
        .field("Guest-linear address                           ", &vmread(vmcs::ro::GUEST_LINEAR_ADDR))
        .finish_non_exhaustive()
    }
}
