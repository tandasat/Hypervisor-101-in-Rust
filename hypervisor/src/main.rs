#![doc = include_str!("../README.md")]
#![no_main]
#![no_std]
#![feature(core_intrinsics)]
#![feature(new_uninit)]
#![feature(panic_info_message)]
#![warn(
    // groups: https://doc.rust-lang.org/rustc/lints/groups.html
    future_incompatible,
    let_underscore,
    nonstandard_style,
    rust_2018_compatibility,
    rust_2018_idioms,
    rust_2021_compatibility,
    unused,

    // warnings that are not enabled by default or covered by groups
    // https://doc.rust-lang.org/rustc/lints/listing/allowed-by-default.html
    macro_use_extern_crate,
    meta_variable_misuse,
    missing_abi,
    missing_copy_implementations,
    missing_debug_implementations,
    missing_docs,
    non_ascii_idents,
    noop_method_call,
    single_use_lifetimes,
    trivial_numeric_casts,
    unreachable_pub,
    unsafe_op_in_unsafe_fn,
    unused_crate_dependencies,
    unused_import_braces,
    unused_lifetimes,
    unused_qualifications,
    unused_results,

    // https://github.com/rust-lang/rust-clippy/blob/master/README.md
    clippy::pedantic,
    clippy::cargo,

    // https://doc.rust-lang.org/rustdoc/lints.html
    rustdoc::missing_crate_level_docs,
    rustdoc::private_doc_tests,
    rustdoc::invalid_html_tags,
)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(not(target_pointer_width = "64"))]
compile_error!("This project must target the 64bit-width pointer environment.");

extern crate alloc;

mod allocator;
mod config;
mod corpus;
mod disk;
mod global_state;
mod hardware_vt;
mod hypervisor;
mod logger;
mod mutation_engine;
mod panic;
mod patch;
mod shell;
mod snapshot;
mod stats;
mod system_table;
mod vm;
mod x86_instructions;

use crate::{
    global_state::GlobalState,
    logger::init_uart_logger,
    system_table::{init_system_table, system_table},
};
use core::ffi::c_void;
use hypervisor::start_hypervisor;
use log::{debug, error, info};
use system_table::system_table_unsafe;
use uefi::{
    prelude::*,
    proto::{loaded_image::LoadedImage, pi::mp::MpServices},
    table::boot::{OpenProtocolAttributes, OpenProtocolParams},
};
use x86::current::paging::{BASE_PAGE_SHIFT, BASE_PAGE_SIZE};

/// The entry point of the program.
#[no_mangle]
extern "efiapi" fn efi_main(image: Handle, system_table: SystemTable<Boot>) -> Status {
    // Initialize the logger and the system services.
    init_uart_logger();
    info!("rhv loaded🔥");

    init_system_table(system_table, image);
    print_image_info();

    // Get command line parameters.
    let args = shell::get_args();
    debug!("Parameters: {args:?}");
    if args.len() != 4 {
        error!("Usage> rhv.efi <snapshot_file> <patch_file> <corpus_dir>");
        return Status::INVALID_PARAMETER;
    }

    let snapshot_path = args[1].as_str();
    let patch_path = args[2].as_str();
    let corpus_path = args[3].as_str();

    // Initialize the global state and start the hypervisor on all logical
    // processors.
    match GlobalState::new(snapshot_path, patch_path, corpus_path) {
        Ok(mut global) => start_hypervisor_on_all_processors(&mut global),
        Err(err) => {
            error!("{err:#?}");
            err.status()
        }
    }
}

/// Starts the hypervisor with [`start_hypervisor`] on all logical processors.
fn start_hypervisor_on_all_processors(global: &mut GlobalState) -> ! {
    if global.number_of_cores() == 1 {
        start_hypervisor(global)
    } else {
        // Run `start_hypervisor_on_ap` on all application processors.
        // Safety: Code is single threaded.
        let st = unsafe { system_table_unsafe() };
        let bs = st.boot_services();
        let mp = unsafe {
            bs.open_protocol::<MpServices>(
                OpenProtocolParams {
                    handle: bs.get_handle_for_protocol::<MpServices>().unwrap(),
                    agent: bs.image_handle(),
                    controller: None,
                },
                OpenProtocolAttributes::GetProtocol,
            )
        }
        .unwrap();

        // NOTE: We lose the current processor. EFI_MP_SERVICES_STARTUP_ALL_APS
        // (== startup_all_aps) cannot be used in the non-blocking mode at this
        // stage, and `start_hypervisor` never returns. So, this API never returns
        // either, and the calling processor is stuck at here. We could fix this
        // by sending INIT-SIPI-SIPI manually.
        let procedure_argument = (global as *mut GlobalState).cast::<c_void>();
        mp.startup_all_aps(false, start_hypervisor_on_ap, procedure_argument, None)
            .unwrap();
        panic!("Should not return from startup_all_aps()")
    }
}

/// Wraps the call to [`start_hypervisor`].
extern "efiapi" fn start_hypervisor_on_ap(context: *mut c_void) {
    let global = unsafe { context.cast::<GlobalState>().as_ref().unwrap() };
    start_hypervisor(global);
}

/// Debug prints the address of this module.
fn print_image_info() {
    let st = system_table();
    let bs = st.boot_services();
    // Safety: The protocol and handle remain valid indefinitely.
    let loaded_image = unsafe {
        bs.open_protocol::<LoadedImage>(
            OpenProtocolParams {
                handle: bs.image_handle(),
                agent: bs.image_handle(),
                controller: None,
            },
            OpenProtocolAttributes::GetProtocol,
        )
        .unwrap()
    };
    let (image_base, image_size) = loaded_image.info();
    info!("rhv image range {:#x} - {:#x}", image_base as u64, image_base as u64 + image_size);
}

/// The structure representing a single memory page (4KB).
//
// This does not _always_ have to be allocated at the page aligned address, but
// very often it is, so let us specify the alignment.
#[derive(Debug, Clone, Copy)]
#[repr(C, align(4096))]
struct Page([u8; BASE_PAGE_SIZE]);
const _: () = assert!(core::mem::size_of::<Page>() == 0x1000);

impl Page {
    fn new() -> Self {
        Self([0; BASE_PAGE_SIZE])
    }
}

/// Computes how many pages are needed for the given bytes.
fn size_to_pages(size: usize) -> usize {
    const PAGE_MASK: usize = 0xfff;

    (size >> BASE_PAGE_SHIFT) + usize::from((size & PAGE_MASK) != 0)
}
