//! The module containing the [`panic_handler`] function.

use crate::x86_instructions::{cli, hlt};
use alloc::string::ToString;
use log::error;

#[panic_handler]
fn panic_handler(info: &core::panic::PanicInfo<'_>) -> ! {
    if let Some(location) = info.location() {
        error!(
            "panicked at '{}', {}:{}:{}",
            info.message().to_string(),
            location.file(),
            location.line(),
            location.column()
        );
    }
    loop {
        // Stop execution of the current processor as much as possible.
        cli();
        hlt();
    }
}
