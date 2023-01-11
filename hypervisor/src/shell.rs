//! The module containing the [`get_args`] function.

// Wraps UEFI Shell protocols to gain command line parameters specified by an
// user. Modern UEFI implements EFI_SHELL_PARAMETERS_PROTOCOL and not the other,
// while some older system such as VMware UEFI implements EFI_SHELL_INTERFACE
// and not the other.
use crate::system_table::system_table_unsafe;
use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use core::ffi::c_void;
use uefi::{
    proto::{loaded_image::LoadedImage, Protocol},
    table::boot::{OpenProtocolAttributes, OpenProtocolParams},
    Char16, Handle,
};

/// Gets argc/argv using `EFI_SHELL_INTERFACE` or
/// `EFI_SHELL_PARAMETERS_PROTOCOL`. <https://github.com/tianocore/edk2/blob/7c0ad2c33810ead45b7919f8f8d0e282dae52e71/ShellPkg/Library/UefiShellCEntryLib/UefiShellCEntryLib.c>
pub(crate) fn get_args() -> Vec<String> {
    match get_args_with_protocol::<ShellInterface>() {
        Ok(args) => args,
        Err(_) => get_args_with_protocol::<ShellParametersProtocol>().unwrap(),
    }
}

// Gets argc/argv using the given protocol.
fn get_args_with_protocol<T: ShellProtocol + Protocol>() -> uefi::Result<Vec<String>> {
    // Safety: Code is single threaded.
    let st = unsafe { system_table_unsafe() };
    let bs = st.boot_services();
    let shell = unsafe {
        bs.open_protocol::<T>(
            OpenProtocolParams {
                handle: bs.image_handle(),
                agent: bs.image_handle(),
                controller: None,
            },
            OpenProtocolAttributes::GetProtocol,
        )?
    };
    Ok(shell.args())
}

// This protocol holds command line parameters.
trait ShellProtocol {
    fn args(&self) -> Vec<String>;
}

// `EFI_SHELL_INTERFACE`
// <https://github.com/tianocore/edk2/blob/7c0ad2c33810ead45b7919f8f8d0e282dae52e71/ShellPkg/Include/Protocol/EfiShellInterface.h#L84>
#[repr(C)]
#[uefi::proto::unsafe_protocol("47c7b223-c42a-11d2-8e57-00a0c969723b")]
struct ShellInterface {
    image_handle: Handle,
    info: *const LoadedImage,
    argv: *const *const Char16,
    argc: usize,
    redir_argv: *const *const Char16,
    redir_argc: usize,
    stdin: *const c_void,
    stdout: *const c_void,
    stderr: *const c_void,
    arg_info: *const u32,
    echo_on: bool,
}

impl ShellProtocol for ShellInterface {
    fn args(&self) -> Vec<String> {
        unsafe {
            let raw_args = core::slice::from_raw_parts(self.argv, self.argc);
            raw_args
                .iter()
                .map(|arg| uefi::CStr16::from_ptr(*arg).to_string())
                .collect()
        }
    }
}

// `EFI_SHELL_PARAMETERS_PROTOCOL`
// <https://github.com/tianocore/edk2/blob/7c0ad2c33810ead45b7919f8f8d0e282dae52e71/MdePkg/Include/Protocol/ShellParameters.h#L50>
#[repr(C)]
#[uefi::proto::unsafe_protocol("752f3136-4e16-4fdc-a22a-e5f46812f4ca")]
struct ShellParametersProtocol {
    argv: *const *const Char16,
    argc: usize,
    stdin: *const c_void,
    stdout: *const c_void,
    stderr: *const c_void,
}

impl ShellProtocol for ShellParametersProtocol {
    fn args(&self) -> Vec<String> {
        unsafe {
            let raw_args = core::slice::from_raw_parts(self.argv, self.argc);
            raw_args
                .iter()
                .map(|arg| uefi::CStr16::from_ptr(*arg).to_string())
                .collect()
        }
    }
}
