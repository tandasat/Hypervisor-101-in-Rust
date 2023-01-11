//! The module containing types and functions to apply and revert patches.

use crate::{
    disk::{open_file, read_file_to_vec},
    Page,
};
use alloc::vec::Vec;
use log::{error, info, trace};
use serde::{Deserialize, Serialize};
use uefi::proto::media::file::Directory;
use x86::current::paging::BASE_PAGE_SHIFT;

/// The collection of [`PatchEntry`]. See also README.md.
#[derive(Debug, Serialize, Deserialize)]
#[allow(clippy::unsafe_derive_deserialize)]
pub(crate) struct PatchSet {
    entries: Vec<PatchEntry>,
}

impl PatchSet {
    /// Creates [`PatchSet`] from the patch file specified by `patch_path`.
    pub(crate) fn new(dir: &mut Directory, patch_path: &str) -> Result<Self, uefi::Error> {
        let mut patch_file = open_file(dir, patch_path)?;
        // Safety: Code is single threaded.
        let contents = unsafe { read_file_to_vec(&mut patch_file) }?;

        info!("Parsing {patch_path:#?}");
        let mut patch_set: PatchSet =
            serde_json::from_slice(contents.as_slice()).map_err(|err| {
                error!("The patch file is corrupted: {err:#?}");
                uefi::Status::DEVICE_ERROR
            })?;
        patch_set.entries.sort_by(|a, b| a.address.cmp(&b.address));

        info!("Patch entry count {}", patch_set.entries.len());
        if !patch_set.entries.is_empty() {
            info!(
                "Patch range {:#x} - {:#x}",
                patch_set.entries.first().unwrap().address,
                patch_set.entries.last().unwrap().address
            );
        }

        Ok(patch_set)
    }

    /// Applies patches for the given PFN if any.
    pub(crate) fn apply(&self, pfn: usize, page: &mut Page) {
        // Find `PatchEntry`s that are within the page specified by `pfn`.
        // `self.entries` is sorted so the range (low and high indexes) can be
        // efficiently searched with `partition_point`.
        let to_pfn = |address| address as usize >> BASE_PAGE_SHIFT;
        let low = self.entries.partition_point(|e| to_pfn(e.address) < pfn);
        let high = self.entries.partition_point(|e| to_pfn(e.address) <= pfn);

        // Apply found patches for this page if any.
        self.entries[low..high].iter().for_each(|entry| {
            let page_offset = (entry.address & 0xfff) as usize;
            let length = entry.length;
            let patch = entry.patch.to_le_bytes();
            page.0[page_offset..page_offset + length].copy_from_slice(&patch[..length]);
        });
        if !self.entries[low..high].is_empty() {
            trace!("Patch applied at {} locations", self.entries[low..high].len());
        }
    }

    /// Finds a patch entry corresponds to the address specified by `rip`.
    pub(crate) fn find(&self, rip: u64) -> Option<&PatchEntry> {
        self.entries.iter().find(|e| e.address == rip)
    }
}

/// The patch entry describing GPA and contents of the patch, as well as
/// original bytes to restore when reverting the patch.
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct PatchEntry {
    address: u64,
    length: usize,
    patch: u32,
    original: u32, // used only when `patch` is 0xCC
}

impl PatchEntry {
    /// Reverts the patch by rewriting the GPA with the original bytes.
    pub(crate) fn revert(&self, snapshot: &mut [Page]) {
        // The following code may concurrently modify the shared resources, ie,
        // snapshot, but there will be no modification that conflicts with other
        // processors, so we are good without lock.
        let pfn = self.address >> BASE_PAGE_SHIFT;
        let page = &mut snapshot[pfn as usize];
        let page_offset = (self.address & 0xfff) as usize;
        let length = self.length;
        let original = self.original.to_le_bytes();

        // Rewrite the patched address with the original bytes
        page.0[page_offset..page_offset + length].copy_from_slice(&original[..length]);
    }
}
