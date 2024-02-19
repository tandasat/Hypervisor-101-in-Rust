//! The module containing [`MutationEngine`] and [`MutatingInput`] types.

use crate::{
    config::MAX_ITERATION_COUNT_PER_FILE,
    corpus::{Corpus, InputFile},
    global_state::GlobalState,
    x86_instructions::rdtsc,
    Page,
};
use alloc::{boxed::Box, format};
use core::{fmt, ptr::addr_of, sync::atomic::AtomicU64};
use log::debug;

/// The context structure representing input data per logical processor.
pub(crate) struct MutationEngine {
    /// State of mutation for the current iteration.
    pub(crate) current_input: MutatingInput,
    /// The address of the first physical address page that backs a copy of the
    /// current input file. Data in this region is mutated and exposed to the
    /// guest.
    input_pages: Box<[Page]>,
}

impl MutationEngine {
    pub(crate) fn new(corpus: &Corpus) -> Self {
        let count = corpus.data_pages().len();
        let input_pages = unsafe { Box::<[Page]>::new_zeroed_slice(count).assume_init() };

        Self {
            current_input: MutatingInput::default(),
            input_pages,
        }
    }

    /// Maps the input data into the guest memory and modifies its contents for
    /// fuzzing.
    pub(crate) fn map_and_mutate_input(
        &mut self,
        corpus: &Corpus,
        active_thread_count: &AtomicU64,
    ) {
        if self.current_input.is_done() {
            // If no more mutation is possible, pick up the new input. In this
            // case, run the guest without mutation first as a baseline.
            let input = if cfg!(feature = "random_byte_modification") {
                corpus.select_file()
            } else {
                corpus.consume_file(active_thread_count)
            };
            self.copy_input_to_guest_memory(&input, corpus.data_gva());
            self.current_input = MutatingInput::new(input);
        } else {
            // Otherwise, mutate the input.
            self.mutate_input();
        }
    }

    // Returns a pointer to the page corresponds to `pfn` from input data.
    fn resolve_page(&self, pfn: usize) -> *const Page {
        addr_of!(self.input_pages[pfn])
    }

    // Copies the immutable input file data into the input data pages.
    fn copy_input_to_guest_memory(&mut self, input: &InputFile, input_data_gva: u64) {
        // Zero clear the input data pages.
        let input_pages = self.input_pages.as_mut();
        input_pages.iter_mut().for_each(|page| page.0.fill(0));

        // Copy the contents of the input file into the input data pages.
        let input_page_addr = input_pages.as_mut_ptr().cast::<u8>();
        let input_data_addr = input.data.as_ptr();
        unsafe {
            core::ptr::copy_nonoverlapping(input_data_addr, input_page_addr, input.data.len());
        };

        // Print debug information
        debug!("Input name {:?}", input.name);
        debug!("Input size {:#x}", input.data.len());
        debug!("Required iteration {}", input.data.len() * 8);
        debug!(
            "Guest accessible input placed at GPA {:#x} - {:#x}, PA {:#x} - {:#x}",
            input_data_gva as usize,
            input_data_gva as usize + input.data.len(),
            input_page_addr as usize,
            input_page_addr as usize + input.data.len(),
        );
        debug!(
            "Original input placed at PA {:#x} - {:#x}",
            input_data_addr as usize,
            input_data_addr as usize + input.data.len()
        );
    }

    // Mutates input data in the input data pages.
    fn mutate_input(&mut self) {
        if cfg!(feature = "random_byte_modification") {
            self.byte_change_input();
        } else {
            self.bit_flip_input();
        }

        self.current_input.mutation_count += 1;
    }

    // Mutates input data in the input data pages with random manner.
    fn byte_change_input(&mut self) {
        let input_pages = unsafe {
            core::slice::from_raw_parts_mut(
                self.input_pages.as_mut_ptr().cast::<u8>(),
                self.current_input.input.data.len(),
            )
        };

        // Restore previous mutation if any.
        if self.current_input.mutation_count >= 1 {
            for i in 0..self.current_input.max_mutation_count {
                let mutation_offset = self.current_input.offsets[i];
                input_pages[mutation_offset] = self.current_input.original[i];
            }
        }

        // Mutate a byte at random locations with random bytes (0x00..0xff).
        self.current_input.max_mutation_count =
            1 + rdtsc() as usize % self.current_input.offsets.len();
        for i in 0..self.current_input.max_mutation_count {
            let mutation_offset = rdtsc() as usize % input_pages.len();
            self.current_input.offsets[i] = mutation_offset;
            self.current_input.original[i] = input_pages[mutation_offset];
            input_pages[mutation_offset] = rdtsc() as u8;
        }
    }

    // Mutates input data in the input data pages with bit flipping.
    fn bit_flip_input(&mut self) {
        let input_pages = self.input_pages.as_mut();

        // Locate the bit position in the snapshot to flip a bit, and do it.
        let page_offset = self.current_input.mutation_count / 8 / 4096;
        let byte_offset = self.current_input.mutation_count / 8 % 4096;
        let bit_offset = self.current_input.mutation_count % 8;
        let input_page = &mut input_pages[page_offset as usize];
        input_page.0[byte_offset as usize] ^= 1 << bit_offset;

        // Restore previous mutation if any.
        if self.current_input.mutation_count >= 1 {
            let prev_page_offset = (self.current_input.mutation_count - 1) / 8 / 4096;
            let prev_byte_offset = (self.current_input.mutation_count - 1) / 8 % 4096;
            let prev_bit_offset = (self.current_input.mutation_count - 1) % 8;
            let prev_input_page = &mut input_pages[prev_page_offset as usize];
            prev_input_page.0[prev_byte_offset as usize] ^= 1 << prev_bit_offset;
        }
    }
}

/// Resolves the PA that should map the given guest pfn within the input data
/// pages.
pub(crate) fn resolve_page_from_input_data(
    global: &GlobalState,
    pfn: usize,
    mutation_engine: &MutationEngine,
) -> Option<*const Page> {
    let pages = global.corpus().data_pages();
    if pages.contains(&pfn) {
        let pfn_in_input_range = pfn - global.corpus().data_pages().start;
        Some(mutation_engine.resolve_page(pfn_in_input_range))
    } else {
        None
    }
}

/// The state of mutation for the current iteration.
#[derive(Default)]
pub(crate) struct MutatingInput {
    /// The immutable, current input file. The copy of this contents is
    /// accessible from the guest. This data is not.
    input: InputFile,
    /// The number of iterations performed with the current
    /// [`MutatingInput::input`].
    mutation_count: u64,
    /// The number of modification to be made in the input file for the current
    /// iteration.
    max_mutation_count: usize,
    /// The array of offsets in the input files that are modified in this
    /// iteration.
    offsets: [usize; 8],
    /// The array of original bytes saved before modification in this iteration.
    original: [u8; 8],
    /// Total bit count in [`MutatingInput::input`].
    total_bits: u64,
}

impl MutatingInput {
    fn new(input: InputFile) -> Self {
        let total_bits = input.data.len() as u64 * 8;
        Self {
            input,
            total_bits,
            ..Default::default()
        }
    }

    pub(crate) fn is_mutated(&self) -> bool {
        self.mutation_count != 0
    }

    pub(crate) fn data(&self) -> InputFile {
        InputFile {
            data: self.input.data.clone(),
            name: format!("{}_{}", self.input.name, self.mutation_count),
        }
    }

    pub(crate) fn size(&self) -> u64 {
        self.input.data.len() as u64
    }

    fn is_done(&self) -> bool {
        if cfg!(feature = "random_byte_modification") {
            self.mutation_count == MAX_ITERATION_COUNT_PER_FILE || self.input.data.is_empty()
        } else {
            self.mutation_count == self.total_bits
        }
    }
}

impl fmt::Debug for MutatingInput {
    fn fmt(&self, format: &mut fmt::Formatter<'_>) -> fmt::Result {
        if cfg!(feature = "random_byte_modification") {
            write!(
                format,
                "{:?} (mutation_count:{} offsets:{:?} bytes:{:?})",
                self.input.name, self.max_mutation_count, self.offsets, self.original,
            )
        } else {
            write!(
                format,
                "{:?} #{} (bit {} at offset {:?} bytes)",
                self.input.name,
                self.mutation_count,
                self.mutation_count.saturating_sub(1) % 8,
                self.mutation_count / 8
            )
        }
    }
}
