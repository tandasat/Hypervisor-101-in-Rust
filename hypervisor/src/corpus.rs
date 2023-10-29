//! The module containing the [`Corpus`] type.

use crate::{
    disk::{open_dir, open_file, read_file_to_vec},
    size_to_pages,
    snapshot::Snapshot,
    x86_instructions::rdtsc,
};
use alloc::{string::String, vec, vec::Vec};
use core::{
    ops::Range,
    sync::atomic::{AtomicU64, Ordering},
};
use log::{debug, error, info};
use spin::RwLock;
use uefi::proto::media::file::{Directory, FileAttribute};
use x86::current::paging::BASE_PAGE_SHIFT;

/// A single input file that is used as a template/baseline to mutate from.
///
/// This is immutable once initialized, and not accessible from the guest.
/// Contents of the input file that are accessible from the guest is managed by
/// the `MutatingInput` type.
#[derive(Default, Debug, Clone)]
pub(crate) struct InputFile {
    /// The host-only-visible copy of immutable input data.
    pub(crate) data: Vec<u8>,
    /// The name of input. It is a file name if it is read from a corpus
    /// directory. Otherwise, some symbolic name.
    pub(crate) name: String,
}

/// The singleton data structure containing a list of input files and memory
/// address to map them in the guest memory. See also README.md.
#[derive(Debug)]
pub(crate) struct Corpus {
    /// The list of immutable input files.
    files: RwLock<Vec<InputFile>>,
    /// The base address of the input data pages in guest VA.
    ///
    /// This address is made up by the hypervisor and contains mutated input
    /// data. The guest registers are adjusted to refer to this region for
    /// input to parse.
    data_gva: u64,
    /// The range of the input data pages in PA.
    ///
    /// This size equals to the size of biggest input file in the corpus,
    /// rounded up to the 4KB granularity. For example, if the biggest input
    /// is 4100 bytes, this will be 2 page-size.
    data_pages: Range<usize>,
}

impl Corpus {
    /// Creates the corpus by reads all files from the specified path.
    pub(crate) fn new(
        dir: &mut Directory,
        corpus_path: &str,
        snapshot: &Snapshot,
    ) -> Result<Self, uefi::Error> {
        let input_files = Self::read_files_in_directory(dir, corpus_path)?;

        // Out of all input files, find the biggest one to reserve memory that is
        // large enough to fit it (and any others). This memory region is used to
        // store mutable copy of an input file, which is accessible from the guest.
        let largest = input_files
            .iter()
            .map(|input_file| input_file.data.len())
            .max()
            .ok_or_else(|| {
                error!("{corpus_path:#?} is empty");
                uefi::Status::NOT_FOUND
            })?;

        // The following diagram illustrates the guest physical address layout.
        // The pages following the inaccessible page are called "input data pages" and
        // contains mutating input data.
        //
        //      |                     |
        //      +---------------------+
        //      | Snapshot page[n]    |    << End of original guest physical memory
        //      +---------------------+
        //      | (Inaccessible page) |
        //      +---------------------+    << self.data_gva
        //      | Input data page[0]  |  \
        //      +---------------------+   \
        //      |                     |    |
        //        ~~~~~~~~~~~~~~~~~~~      +< self.data_pages
        //      |                     |    |
        //      +---------------------+   /
        //      | Input data page[n]  |  /
        //      +---------------------+
        //      | (Inaccessible page) |
        //
        let size_in_pages = size_to_pages(largest);
        let input_data_page_first = snapshot.memory.len() + 1;
        let input_data_page_end = input_data_page_first + size_in_pages;
        Ok(Self {
            files: RwLock::new(input_files),
            data_gva: (input_data_page_first << BASE_PAGE_SHIFT) as u64,
            data_pages: input_data_page_first..input_data_page_end,
        })
    }

    /// Returns the base of the guest virtual address that maps the mutated
    /// contents of input files.
    pub(crate) fn data_gva(&self) -> u64 {
        self.data_gva
    }

    /// Returns the range of physical address that maps the mutated contents of
    /// input files.
    pub(crate) fn data_pages(&self) -> Range<usize> {
        self.data_pages.clone()
    }

    /// Returns the number of remaining input files.
    pub(crate) fn remaining_files_count(&self) -> usize {
        self.files.read().len()
    }

    /// Picks up the next input file from the corpus.
    ///
    /// It removes an input file from the corpus. If there is no more input
    /// file, the calling thread will wait until a new input file is added.
    /// If the last active thread enters the wait state, fuzzing is complete
    /// as it panics.
    pub(crate) fn consume_file(&self, active_thread_count: &AtomicU64) -> InputFile {
        let _ = active_thread_count.fetch_sub(1, Ordering::SeqCst);
        let input_file = loop {
            {
                let mut input_files = self.files.write();
                if let Some(input_file) = input_files.pop() {
                    break input_file;
                }
            }
            core::hint::spin_loop();
            assert!(active_thread_count.load(Ordering::SeqCst) > 0, "No more input file");
        };
        let _ = active_thread_count.fetch_add(1, Ordering::SeqCst);

        debug!(
            "Picking up a new input file {:?}. Remaining {}",
            input_file.name,
            self.remaining_files_count()
        );
        input_file
    }

    /// Picks up the next input file from the corpus in a random manner. This
    /// function returns a copy of an input file and keeps the corpus unchanged.
    pub(crate) fn select_file(&self) -> InputFile {
        let input_files = self.files.read();
        let index = rdtsc() as usize % input_files.len();
        input_files[index].clone()
    }

    /// Adds a new input file into the corpus.
    pub(crate) fn add_file(&self, input: InputFile) {
        debug!(
            "Adding a new input file {:?}. Remaining {}",
            input.name,
            self.remaining_files_count() + 1
        );

        self.files.write().push(input);
    }

    // Reads the contents of all files in the specified corpus directory.
    fn read_files_in_directory(
        dir: &mut Directory,
        corpus_path: &str,
    ) -> Result<Vec<InputFile>, uefi::Error> {
        let mut files: Vec<InputFile> = Vec::new();
        let mut corpus_dir = open_dir(dir, corpus_path)?;
        let mut buffer = vec![0; 128];
        loop {
            let file_info = match corpus_dir.read_entry(&mut buffer) {
                Ok(info) => {
                    if let Some(info) = info {
                        info
                    } else {
                        // We've reached the end of the directory
                        break;
                    }
                }
                Err(err) => {
                    // Buffer is not big enough, allocate a bigger one and try again.
                    let min_size = err.data().unwrap();
                    buffer.resize(min_size, 0);
                    continue;
                }
            };

            // Non recursive search for simplicity.
            if file_info.attribute().contains(FileAttribute::DIRECTORY) {
                continue;
            }

            let mut name = String::new();
            file_info
                .file_name()
                .as_str_in_buf(&mut name)
                .map_err(|_err| uefi::Status::INVALID_PARAMETER)?;
            let mut file = open_file(&mut corpus_dir, &name)?;
            // Safety: Code is single threaded.
            let data = unsafe { read_file_to_vec(&mut file) }?;
            info!("Adding an input file {name:?}");
            files.push(InputFile { data, name });
        }
        Ok(files)
    }
}
