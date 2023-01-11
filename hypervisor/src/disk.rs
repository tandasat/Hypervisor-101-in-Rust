//! The module containing functions to access disk.
//!
//! Disk access is done through the `SimpleFileSystem` protocol.
//! See also <https://github.com/tianocore/edk2/blob/master/MdePkg/Include/Protocol/SimpleFileSystem.h>
//! The protocol as well as the Rust-layer are both not thread-safe due to
//! dependency on the UEFI system table. Hence, some functions are serialized
//! internally, and some are marked as `unsafe`.

use crate::{system_table::system_table, Page};
use alloc::{boxed::Box, vec, vec::Vec};
use log::error;
use uefi::proto::media::file::{
    Directory, File, FileAttribute, FileInfo, FileMode, FileType, RegularFile,
};
use x86::current::paging::{BASE_PAGE_SHIFT, BASE_PAGE_SIZE};

/// Opens a file specified by `filename`.
pub(crate) fn open_file(dir: &mut Directory, filename: &str) -> Result<RegularFile, uefi::Error> {
    match open(dir, filename)? {
        FileType::Regular(file) => Ok(file),
        FileType::Dir(_) => {
            error!("{filename:#?} is not a file");
            Err(uefi::Error::from(uefi::Status::INVALID_PARAMETER))
        }
    }
}

/// Opens a directory specified by `dirname`.
pub(crate) fn open_dir(dir: &mut Directory, dirname: &str) -> Result<Directory, uefi::Error> {
    match open(dir, dirname)? {
        FileType::Regular(_) => {
            error!("{dirname:#?} is not a directory");
            Err(uefi::Error::from(uefi::Status::INVALID_PARAMETER))
        }
        FileType::Dir(dir) => Ok(dir),
    }
}

/// Returns the details of the file.
///
/// # Safety
///
/// The caller must ensure no other thread use the UEFI system table
/// concurrently. Implementation calls the global allocator, which uses the UEFI
/// system table.
pub(crate) unsafe fn get_file_info(file: &mut impl File) -> Result<Box<FileInfo>, uefi::Error> {
    file.get_boxed_info::<FileInfo>()
}

/// Reads the whole contents of the file into the vector.
///
/// # Safety
///
/// The caller must ensure no other thread use the UEFI system table
/// concurrently. Implementation calls the global allocator, which uses the UEFI
/// system table.
pub(crate) unsafe fn read_file_to_vec(file: &mut RegularFile) -> Result<Vec<u8>, uefi::Error> {
    let size = unsafe { get_file_info(file) }?.file_size() as usize;
    let mut buf = vec![0; size];

    let _lock = system_table();
    if file
        .read(&mut buf)
        .map_err(|_err| uefi::Status::DEVICE_ERROR)?
        == size
    {
        Ok(buf)
    } else {
        Err(uefi::Error::from(uefi::Status::END_OF_FILE))
    }
}

// Reads a single page from the snapshot file.
pub(crate) fn read_page_from_snapshot(
    snapshot_file: &mut RegularFile,
    page: &mut Page,
    pfn: usize,
) -> Result<(), uefi::Error> {
    // Acquire the UEFI system table lock before use of the file API.
    let _lock = system_table();
    snapshot_file.set_position((pfn << BASE_PAGE_SHIFT) as u64)?;
    let bytes_read = snapshot_file.read(&mut page.0).map_err(|err| {
        error!("File read error: {err:#?}");
        uefi::Status::DEVICE_ERROR
    })?;

    if bytes_read == BASE_PAGE_SIZE {
        Ok(())
    } else {
        Err(uefi::Error::from(uefi::Status::END_OF_FILE))
    }
}

// Opens any kind of "file" specified by `filename`.
fn open(dir: &mut Directory, filename: &str) -> Result<FileType, uefi::Error> {
    const BUF_SIZE: usize = 255;
    let mut buf = [0; BUF_SIZE + 1];
    let name = uefi::CStr16::from_str_with_buf(filename, &mut buf)
        .map_err(|_err| uefi::Status::INVALID_PARAMETER)?;

    // Acquire the UEFI system table lock before use of the file API.
    let _lock = system_table();
    dir.open(name, FileMode::Read, FileAttribute::empty())
        .map_err(|err| {
            error!("{filename:#?}: {:#?}", err.status());
            err
        })?
        .into_type()
}
