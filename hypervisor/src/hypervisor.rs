//! The module containing high-level execution flow of this project.
//!
//! Logic this module implements can be understood as implementation of a
//! hypervisor, the component responsible for configuring and running VMs. This
//! project creates and runs one VM per logical processor, ie, 4 VMs will run
//! concurrently if the system has 4 logical processors.
//!
//! Any code running in and from this module must not exceed 32KB (0x8000) for
//! total stack usage. Application processors run with this much of stack.
//! Overflow silently causes memory corruption. Thus, large structures should be
//! allocated on heap. This is usually not an issue with a single core system
//! because the boot strap processor (ie, the processor 0) runs with 128KB of
//! stack.

use crate::{
    config::GUEST_EXEC_TIMEOUT_IN_TSC,
    global_state::GlobalState,
    hardware_vt::{
        ExceptionQualification, GuestException, NestedPageFaultQualification, VmExitReason,
    },
    mutation_engine::{resolve_page_from_input_data, MutatingInput, MutationEngine},
    snapshot::resolve_page_from_snapshot,
    stats::RunStats,
    vm::Vm,
    x86_instructions::rdtsc,
    Page,
};
use core::sync::atomic::Ordering;
use log::{debug, error, info, trace, warn};
use x86::current::paging::BASE_PAGE_SHIFT;

/// Prepares a VM and enters the infinite fuzzing loop with the VM.
///
/// This function activates hardware-assisted virtualization, configures
/// the hypervisor and VM, and executes the VM with the given corpus
/// semi-indefinitely.
pub(crate) fn start_hypervisor(global: &GlobalState) -> ! {
    info!("Starting the hypervisor");

    // Create an instance of a VM, enable hardware-assisted virtualization, and
    // set up the hypervisor.
    let mut vm = Vm::new();
    vm.vt.enable();
    let nested_pml4_addr = vm.nested_pml4_addr() as u64;
    vm.vt.initialize(nested_pml4_addr);

    // Initialize the component that is responsible for selecting an input file
    // from the corpus and mutating it.
    let mut mutation_engine = MutationEngine::new(global.corpus());

    // Enter the fuzzing loop, that is: running the VM from a snapshot until it
    // aborts, printing out the stats, reverting dirty pages and repeating those.
    info!("Entering the fuzzing loop🐇");
    let _ = global.active_thread_count.fetch_add(1, Ordering::SeqCst);
    loop {
        // Run the VM.
        let (stats, abort_reason) = start_vm(&mut vm, &mut mutation_engine, global);

        // The VM has aborted. Update overall stats, report them and the reason
        // of abort. There are two types of stats: stats about this particular
        // fuzzing iteration (`stats`) and stats about all fuzzing iterations
        // including ones that ran by other logical processors (within `global`).
        let iter_count = global.update_stats(&stats);
        stats.report(global, vm.used_dirty_page_count(), iter_count);
        abort_reason.report(&mutation_engine.current_input);

        // Add the current input file to the corpus if it caused execution of
        // new basic block(s).
        if !stats.newly_executed_basic_blks.is_empty() && mutation_engine.current_input.is_mutated()
        {
            global
                .corpus()
                .add_file(mutation_engine.current_input.data());
        }
    }
}

/// Runs a fuzzing iteration and returns stats and a reason of the end of the
/// iteration.
///
/// This function resets the VM based on the snapshot, mutates input data,
/// and runs the VM until it encounters one of abort conditions.
fn start_vm(
    vm: &mut Vm,
    mutation_engine: &mut MutationEngine,
    global: &GlobalState,
) -> (RunStats, AbortReason) {
    // Configure the VM based on the snapshot. Memory is paged-in from snapshot
    // on nested page fault. `revert_dirty_memory` only reverts pages that are
    // already paged in AND modified by the guest in the previous iteration.
    vm.revert_dirty_memory();
    vm.vt.revert_registers(&global.snapshot());

    // Inject mutated input data into VM's memory.
    mutation_engine.map_and_mutate_input(global.corpus(), &global.active_thread_count);

    // Update VM's registers to point to the mutated input data.
    vm.vt
        .adjust_registers(global.corpus().data_gva(), mutation_engine.current_input.size());

    // Run the VM until it reaches one of abort conditions.
    let stats = &mut RunStats::new();
    loop {
        // Run the VM until VM exit happens.
        let exit_reason = vm.vt.run();

        // VM exit happened and execution of the VM is suspended. The hypervisor
        // needs to handle VM exit according to `exit_reason`.
        let host_start_tsc = rdtsc();
        let exit_handling_result = match exit_reason {
            VmExitReason::NestedPageFault(qualification) => {
                handle_nested_page_fault(vm, global, mutation_engine, &qualification)
            }
            VmExitReason::Exception(qualification) => {
                handle_interrupt_or_exception(global, stats, &qualification)
            }
            VmExitReason::ExternalInterruptOrPause => handle_external_interrupt_or_pause(stats),
            VmExitReason::TimerExpiration => handle_timer_expiration(stats),
            VmExitReason::Shutdown(exit_code) => VmExitResult::Panic(exit_code),
            VmExitReason::Unexpected(exit_code) => {
                error!("🐈 Unhandled VM exit {exit_code:#x}");
                VmExitResult::AbortVm(AbortReason::UnhandledVmExit)
            }
        };
        stats.vmexit_count += 1;
        stats.host_spent_tsc += rdtsc() - host_start_tsc;

        // Either resume the VM, abort the VM, or panic the hypervisor according
        // to the result of VM exit handling.
        match exit_handling_result {
            VmExitResult::ResumeVm => continue,
            VmExitResult::AbortVm(reason) => {
                // An abort condition reached. Return the stats and reason.
                stats.total_tsc = rdtsc() - stats.start_tsc;
                return (stats.clone(), reason);
            }
            VmExitResult::Panic(exit_code) => {
                error!("{:#x?}", vm.vt);
                panic!("🐛 Non continuable VM exit {exit_code:#x}");
            }
        }
    }
}

/// Handles VM exit due to nested page fault.
///
/// This happens for three major reasons:
/// 1. The VM started without any memory being mapped. Any VM's attempt to
///    access memory fails due to missing GPA -> PA translation. This function
///    builds GPA -> PA address translation on the fly. Once translation is
///    built, that is used indefinitely and not cleared at the end of iteration.
/// 2. VM's memory is mapped as read-only. Any newly mapped memory through (1)
///    is read-only, and any VM's attempt to write to it will fail due to
///    permission violation. This function performs copy-on-write and allows
///    further write access within this iteration. At the end of iteration, all
///    "dirty" pages are discarded with [`Vm::revert_dirty_memory`].
/// 3. The VM accesses memory that is not captured in the snapshot. This is
///    possible and common because of MMIO. In this situation,
///    [`resolve_pa_for_gpa`] fails, and this function returns
///    [`VmExitResult::AbortVm`] to abort the VM. This is the most common reason
///    of aborting the VM.
fn handle_nested_page_fault(
    vm: &mut Vm,
    global: &GlobalState,
    mutation_engine: &MutationEngine,
    qualification: &NestedPageFaultQualification,
) -> VmExitResult {
    if global.iter_count() == 0 {
        trace!("{qualification:x?}");
    }

    // Resolve a PA that maps or will map the GPA that the guest tried to access.
    // This works as follows:
    // 1. If the GPA is within the snapshot, the GPA should be backed by a page in
    //    the snapshot.
    // 2. If the GPA is outside the snapshot but within the input data pages, the
    //    GPA should be backed by the input data pages.
    let gpa = qualification.gpa as usize;
    let pa = match resolve_pa_for_gpa(gpa, mutation_engine, global) {
        Ok(pa) => pa,
        Err(err) => return err,
    };

    // If this VM exit is due to missing GPA -> PA translation, build GPA -> PA
    // translation. Note that the PA resolved by `resolve_pa_for_gpa` is either
    // in the snapshot or an input file, and contents of the snapshot is shared
    // across all VMs. VMs should never be able to modify that, or changes made
    // by one VM would be visible from other VMs. We enforces this restriction
    // via copy-on-write mechanism (see below).
    if qualification.missing_translation {
        // Instruction: Uncomment and complete implementation of it.
        vm.build_translation(gpa, pa);
    }

    // If this is a write memory access, trigger copy-on-write. That is, with
    // `copy_on_write`, update GPA -> PA translation to map the GPA to one of
    // preallocated dirty pages instead of a snapshot or an input file, `pa`.
    // Then, copy current contents of memory at `pa` to the new dirty page. This
    // effectively isolate the effect of memory write into this current guest.
    // Failure of copy-on-write warrants aborting the VM.
    warn!("E#6-2");
    // Instruction: Enable copy-on-write semantic by
    //              1. updating nested translation for `gpa` to use separate
    //                 dirty pages, then
    //              2. copying current contents of the PA that maps `gpa` into
    //                 the dirty page selected
    // Use: qualification.write_access, vm.copy_on_write()

    // Since we changed nested paging structure entries, cache invalidation may be
    // required.
    vm.vt.invalidate_caches();
    VmExitResult::ResumeVm
}

/// Returns the physical address that backs the GPA specified by `gpa`.
///
/// This function checks if the GPA is within the snapshot or the input data
/// pages. If so, returns a PA within those. Otherwise, returns [`Err`].
fn resolve_pa_for_gpa(
    gpa: usize,
    mutation_engine: &MutationEngine,
    global: &GlobalState,
) -> Result<*const Page, VmExitResult> {
    let pfn = gpa >> BASE_PAGE_SHIFT;

    // If the GPA being accessed is captured within the snapshot, resolve the
    // page from the snapshot. If not, check if it is within the input data pages.
    if let Some(page) = resolve_page_from_snapshot(global, pfn) {
        Ok(page)
    } else if let Some(page) = resolve_page_from_input_data(global, pfn, mutation_engine) {
        Ok(page)
    } else if pfn == 0 {
        Err(VmExitResult::AbortVm(AbortReason::NullPageAccess))
    } else if pfn == 0xf_ffff_ffff_ffff {
        Err(VmExitResult::AbortVm(AbortReason::NegativePageAccess))
    } else {
        // Access to the outside of any guest physical memory ranges. This can be
        // normal due to MMIO.
        //
        // NOTE: We should detect if this is actually within MMIO regions or random
        // memory access as a result of triggering a bug. We could do that by capturing
        // MMIO physical memory ranges within the snapshot, although enumerating those
        // ranges most likely require platform specific API calls.
        Err(VmExitResult::AbortVm(AbortReason::InvalidPageAccess))
    }
}

/// Handles VM exit due to exceptions happened in the VM.
///
/// Those can happen because of our patch (eg, 0xCC) or a bug discovered by
/// fuzzing. This function determines the cause and recovers or aborts the VM.
fn handle_interrupt_or_exception(
    global: &GlobalState,
    stats: &mut RunStats,
    qualification: &ExceptionQualification,
) -> VmExitResult {
    todo!("E#7-2");
    // Instruction: Comment out this todo!(). Make sense of execution flow to reach
    //              here. What VM exit code was observed at the vmx.rs or svm.rs
    //              level?
    /*
    assert!(qualification.exception_code == GuestException::InvalidOpcode);
    return VmExitResult::AbortVm(AbortReason::EndMarker);
    */

    todo!("E#8-2");
    // Instruction: 1. Comment out the above two lines.
    //              2. Uncomment the following match-block.
    //              3. Implement handling of `GuestException::BreakPoint` by:
    //                  1. reverting patches in the snapshot memory
    //                  2. saving the guest RIP as coverage information
    //                  3. returning ResumeVm
    // Hint: (1) entry.revert(), global.snapshot_mut().memory
    //       (2) stats.newly_executed_basic_blks.push(), qualification.rip
    //       (3) VmExitResult::ResumeVm
    /*
    match global.patch_set().find(qualification.rip) {
        // There is a patch entry for RIP.
        Some(entry) => match qualification.exception_code {
            // If this is #BP, the exception is because of our coverage tracking
            // patch. Revert the patch, increase coverage, and resume the VM.
            GuestException::BreakPoint => {

            }
            // If this is #UD, it is our end marker. Abort the VM. This is the most
            // common abort reason.
            GuestException::InvalidOpcode => VmExitResult::AbortVm(AbortReason::EndMarker),
            // If this is #PF, it may be a bug found by fuzzing. Abort the VM.
            GuestException::PageFault => VmExitResult::AbortVm(AbortReason::UnexpectedPageFault),
        },

        // There is no patch entry for RIP. Exception is not because of the patch.
        // Abort the VM.
        None => match qualification.exception_code {
            GuestException::BreakPoint => VmExitResult::AbortVm(AbortReason::UnexpectedBreakpoint),
            GuestException::InvalidOpcode => VmExitResult::AbortVm(AbortReason::InvalidInstruction),
            GuestException::PageFault => VmExitResult::AbortVm(AbortReason::UnexpectedPageFault),
        },
    }
    */
}

/// Handles VM exit due to external interrupt, such as timer interrupt, or
/// `PAUSE`.
///
/// This functions determines if the quantum given to the VM has expired.
fn handle_external_interrupt_or_pause(stats: &mut RunStats) -> VmExitResult {
    let total_elapsed_tsc = rdtsc() - stats.start_tsc;
    let guest_spent_tsc = total_elapsed_tsc - stats.host_spent_tsc;
    if guest_spent_tsc < GUEST_EXEC_TIMEOUT_IN_TSC {
        VmExitResult::ResumeVm
    } else {
        handle_timer_expiration(stats)
    }
}

/// Handles VM exit due to expiration of the quantum given to the VM.
fn handle_timer_expiration(stats: &mut RunStats) -> VmExitResult {
    stats.hang_count = 1;
    VmExitResult::AbortVm(AbortReason::Hang)
}

/// The result of handing VM exit.
enum VmExitResult {
    /// The VM should resume and retry the same instruction.
    ResumeVm,
    /// The VM should abort, and the new fuzzing iteration should start.
    AbortVm(AbortReason),
    /// The current processor should panic.
    Panic(u64),
}

/// The detailed reason of [`VmExitResult::AbortVm`].
enum AbortReason {
    /// The VM caused VM exit that is not handled.
    /// Source: [`VmExitReason::Unexpected`].
    UnhandledVmExit,

    /// The VM reached to the end marker UD instruction.
    /// Source: [`VmExitReason::Exception`].
    EndMarker,

    /// The VM attempted to access memory that is not backed by the snapshot or
    /// input data. Source: [`VmExitReason::NestedPageFault`].
    InvalidPageAccess,

    /// The VM attempted to access the null page. An indicator of a bug.
    /// Source: [`VmExitReason::NestedPageFault`].
    NullPageAccess,

    /// The VM attempted to access address -1 (0xfffffff....). An indicator of a
    /// bug. Source: [`VmExitReason::NestedPageFault`].
    NegativePageAccess,

    /// The VM attempted to execute an invalid instruction. An indicator of a
    /// bug. Source: [`VmExitReason::Exception`].
    InvalidInstruction,

    /// The VM attempted to execute a breakpoint instruction that is not
    /// originated by the patch. An indicator of a bug.
    /// Source: [`VmExitReason::Exception`].
    UnexpectedBreakpoint,

    /// The VM generated #PF, which is not expected with _our snapshot_, which
    /// is taken at the UEFI phase. Maybe a bug.
    /// Source: [`VmExitReason::Exception`].
    UnexpectedPageFault,

    /// The VM has modified too many pages. Maybe a bug.
    /// Source: [`VmExitReason::NestedPageFault`].
    ExcessiveMemoryWrite,

    /// The VM has used up its quantum. Maybe a bug.
    /// Source: [`VmExitReason::ExternalInterruptOrPause`] or
    /// [`VmExitReason::TimerExpiration`] .
    Hang,
}

impl AbortReason {
    /// Prints out the reason of abort if needed.
    ///
    /// Those may be indicators of bugs found as a result of fuzzing are
    /// reported as warning.
    fn report(&self, current_input: &MutatingInput) {
        match self {
            Self::UnhandledVmExit | Self::InvalidPageAccess => (),
            Self::EndMarker => trace!("Reached the end marker"),
            Self::NullPageAccess => warn!("NULL PAGE ACCESS : {current_input:?}"),
            Self::NegativePageAccess => warn!("NEGATIVE PAGE ACCESS : {current_input:?}"),
            Self::InvalidInstruction => warn!("INVALID INSTRUCTION : {current_input:?}"),
            Self::UnexpectedBreakpoint => warn!("UNEXPECTED BREAKPOINT : {current_input:?}"),
            Self::UnexpectedPageFault => warn!("UNEXPECTED PAGE FAULT : {current_input:?}"),
            Self::ExcessiveMemoryWrite => warn!("EXCESSIVE MEMORY WRITES : {current_input:?}"),
            Self::Hang => debug!("Hang detected : {current_input:?}"),
        }
    }
}

impl From<GuestException> for AbortReason {
    /// Converts [`GuestException`] to [`AbortReason`].
    fn from(value: GuestException) -> Self {
        match value {
            GuestException::BreakPoint => Self::UnexpectedBreakpoint,
            GuestException::InvalidOpcode => Self::InvalidInstruction,
            GuestException::PageFault => Self::InvalidPageAccess,
        }
    }
}
