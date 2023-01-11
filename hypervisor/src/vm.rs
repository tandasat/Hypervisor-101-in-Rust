//! The module containing the [`Vm`] type.

use crate::{
    hardware_vt::{
        svm::Svm, vmx::Vmx, HardwareVt, NestedPagingStructure, NestedPagingStructureEntry,
        NestedPagingStructureEntryType,
    },
    Page,
};
use alloc::boxed::Box;
use core::ptr::addr_of;
use log::trace;
use x86::current::paging::BASE_PAGE_SHIFT;

/// The representation of a virtual machine, made up of collection of registers,
/// which is managed through [`HardwareVt`], preallocated
/// [`NestedPagingStructure`]s to build GPA -> PA translations, and preallocated
/// dirty [`Page`]s to back GPAs that are modified by the VM.
pub(crate) struct Vm {
    /// Encapsulates implementation of hardware assisted virtualization
    /// technology, which is capable of managing VM's registers and memory.
    pub(crate) vt: Box<dyn HardwareVt>,

    /// The nested PML4. All other nested paging structures are built on the fly
    /// by consuming [`Vm::nested_paging_structures`].
    nested_pml4: Box<NestedPagingStructure>,

    /// Preallocated nested paging structures for dynamically building GPA -> PA
    /// translation.
    nested_paging_structures: Box<[NestedPagingStructure]>,

    /// How many [`Vm::nested_paging_structures`] has been consumed.
    used_nps_count: usize,

    /// Preallocated pages to be used for copy-on-write.
    dirty_pages: Box<[Page]>,

    /// The pairs of modified nested PTEs and original PAs due to copy-on-write.
    dirty_entries: Box<[(*mut NestedPagingStructureEntry, u64)]>,

    /// How many [`Vm::dirty_pages`] has been consumed.
    used_dirty_page_count: usize,
}

impl Vm {
    pub(crate) fn new() -> Self {
        // The number of pre-allocated pages used to back modified pages (ie,
        // dirty pages). The VM can modify up to this number of pages. If the VM
        // attempts to modify more pages than this, the VM is aborted.
        const DIRTY_PAGE_COUNT: usize = 1024;

        // The number of pre-allocated nested paging structures. The more memory the VM
        // accesses, the more tables we need. If the VM attempts to access more
        // memory than this can manage, the hypervisor will panic.
        const NPS_COUNT: usize = 1024;

        // Use VMX on Intel and SMV on AMD.
        let vt: Box<dyn HardwareVt> = if is_intel() {
            trace!("Processor is Intel");
            Box::new(Vmx::new())
        } else {
            trace!("Processor is AMD");
            Box::new(Svm::new())
        };

        let nested_pml4 = unsafe { Box::<NestedPagingStructure>::new_zeroed().assume_init() };

        let nested_paging_structures =
            unsafe { Box::<[NestedPagingStructure]>::new_zeroed_slice(NPS_COUNT).assume_init() };

        let dirty_pages =
            unsafe { Box::<[Page]>::new_zeroed_slice(DIRTY_PAGE_COUNT).assume_init() };

        let dirty_entries = unsafe {
            Box::<[(*mut NestedPagingStructureEntry, u64)]>::new_zeroed_slice(dirty_pages.len())
                .assume_init()
        };

        Self {
            vt,
            nested_pml4,
            nested_paging_structures,
            used_nps_count: 0,
            dirty_pages,
            dirty_entries,
            used_dirty_page_count: 0,
        }
    }

    pub(crate) fn used_dirty_page_count(&self) -> usize {
        self.used_dirty_page_count
    }

    pub(crate) fn nested_pml4_addr(&mut self) -> *mut NestedPagingStructure {
        self.nested_pml4.as_mut() as *mut _
    }

    /// Revert all dirty nested PTEs to point to the original physical
    /// addresses.
    pub(crate) fn revert_dirty_memory(&mut self) {
        // Iterate over all saved dirty PTEs and revert its translations to the
        // original PAes.
        let flags = self
            .vt
            .nps_entry_flags(NestedPagingStructureEntryType::RxWriteBack);
        for i in 0..self.used_dirty_page_count {
            let dirty_entry = &self.dirty_entries[i];
            let dirty_pte = unsafe { dirty_entry.0.as_mut() }.unwrap();
            let original_pa = dirty_entry.1 << BASE_PAGE_SHIFT;
            dirty_pte.set_translation(original_pa, flags);
        }

        // Updating the nested paging structure entries may warrant cache invalidation.
        if self.used_dirty_page_count != 0 {
            self.vt.invalidate_caches();
            self.used_dirty_page_count = 0;
        }
    }

    /// Builds nested paging translation for `gpa` to translate to `pa`.
    ///
    /// This function does so by walking through whole PML4 -> PDPT -> PD -> PT
    /// as a processor does, and allocating tables and initializing table
    /// entries as needed.
    #[allow(clippy::similar_names)]
    pub(crate) fn build_translation(&mut self, gpa: usize, pa: *const Page) {
        let pml4i = (gpa >> 39) & 0b1_1111_1111;
        let pdpti = (gpa >> 30) & 0b1_1111_1111;
        let pdi = (gpa >> 21) & 0b1_1111_1111;
        let pti = (gpa >> 12) & 0b1_1111_1111;

        // Locate PML4, index it, build PML4e as needed
        /*
                                Nested PML4 (4KB)
            nested_pml4_addr -> +-----------+
                                |           | [0]
                                +-----------+
                                |   pml4e   | [1] <----- pml4i (eg = 1)
                                +-----------+
                                |           |
                                     ...
                                |           | [512]
                                +-----------+
            walk_table() does two things if the indexed entry is empty:
                1. allocate a new table (ie, next table) from preallocated buffer
                2. update the entry's pfn to point to the next table (see below diagram)

            pml4e (64bit)
            +-----------------+------+
            |    N:12 (pfn)   | 11:0 |
            +-----------------+------+
                   \                Nested PDPT (4KB)
                    \-------------> +-----------+
                                    |           | [0]
                                    +-----------+
                                    |   pdpte   | [1] <----- pdpti (eg = 1)
                                    +-----------+
                                    |           |
                                        ...
                                    |           | [512]
                                    +-----------+
        */
        let pml4 = unsafe { self.nested_pml4_addr().as_mut() }.unwrap();
        let pml4e = self.walk_table(pml4, pml4i);

        // Locate PDPT, index it, build PDPTe as needed
        let pdpt = pml4e.next_table_mut();
        let pdpte = self.walk_table(pdpt, pdpti);

        // Locate PD, index it, build PDe as needed
        let pd = pdpte.next_table_mut();
        let pde = self.walk_table(pd, pdi);

        // Locate PT, index it, build PTe as needed
        let pt = pde.next_table_mut();
        let pte = &mut pt.entries[pti];
        assert!(pte.0 == 0);

        // Make it non-writable so that copy-on-write is done for dirty pages.
        let flags = self
            .vt
            .nps_entry_flags(NestedPagingStructureEntryType::RxWriteBack);
        pte.set_translation(pa as u64, flags);
    }

    /// Updates nested paging translation for `gpa` to translate to a dirty page
    /// and copies the original contents at `copy_from` into the new dirty page.
    #[allow(clippy::similar_names)]
    pub(crate) fn copy_on_write(&mut self, gpa: usize, copy_from: *const Page) -> bool {
        if self.used_dirty_page_count >= self.dirty_pages.len() {
            return false;
        }

        let pml4i = (gpa >> 39) & 0b1_1111_1111;
        let pdpti = (gpa >> 30) & 0b1_1111_1111;
        let pdi = (gpa >> 21) & 0b1_1111_1111;
        let pti = (gpa >> 12) & 0b1_1111_1111;

        // Locate PML4, index it, build PML4e as needed
        let pml4 = unsafe { self.nested_pml4_addr().as_mut() }.unwrap();
        let pml4e = self.walk_table(pml4, pml4i);

        // Locate PDPT, index it, build PDPTe as needed
        let pdpt = pml4e.next_table_mut();
        let pdpte = self.walk_table(pdpt, pdpti);

        // Locate PD, index it, build PDe as needed
        let pd = pdpte.next_table_mut();
        let pde = self.walk_table(pd, pdi);

        // Locate PT, index it.
        let pt = pde.next_table_mut();
        let pte = &mut pt.entries[pti];

        // Saves nested PTE and the original (current) PA for reverting.
        self.dirty_entries[self.used_dirty_page_count] = (pte as *mut _, pte.pfn());

        // Update translation to point to `dirty_pages`, which is allocated for
        // each logical processor and never be shared with others. Thus, updating
        // nested paging structures (which are also exclusive to each logical processor)
        // and pointing to the dirty page will isolate write access to this guest
        // only. The modified page will be only visible from this guest.
        let flags = self
            .vt
            .nps_entry_flags(NestedPagingStructureEntryType::RwxWriteBack);
        let new_page = &mut self.dirty_pages[self.used_dirty_page_count];
        pte.set_translation(new_page as *const _ as u64, flags);
        self.used_dirty_page_count += 1;

        // Copy contents of the previous physical address into the new physical
        // address.
        unsafe {
            core::ptr::copy_nonoverlapping(copy_from, new_page as *mut _, 1);
        };

        true
    }

    /// Locates a nested paging structure entry from `table` using `index`.
    ///
    /// This function initializes the entry if it is not yet. `table` must be
    /// either a nested PML4, PDPT, or PD. Not PT.
    fn walk_table<'a>(
        &mut self,
        table: &'a mut NestedPagingStructure,
        index: usize,
    ) -> &'a mut NestedPagingStructureEntry {
        let entry = &mut table.entries[index];

        // If there is no information about the next table in the entry, add that.
        // An unused `nested_paging_structures` is used as a next table.
        if entry.0 == 0 {
            assert!(
                self.used_nps_count < self.nested_paging_structures.len(),
                "All preallocated nested paging structures exhausted",
            );
            let next_table = addr_of!(self.nested_paging_structures[self.used_nps_count]) as u64;
            entry.set_translation(
                next_table,
                self.vt.nps_entry_flags(NestedPagingStructureEntryType::Rwx),
            );
            self.used_nps_count += 1;
        }
        entry
    }
}

/// Checks whether the current processor is Intel-processors (as opposed to
/// AMD).
fn is_intel() -> bool {
    x86::cpuid::CpuId::new().get_vendor_info().unwrap().as_str() == "GenuineIntel"
}
