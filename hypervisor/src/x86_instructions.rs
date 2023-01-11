//! The module containing wrapper functions for x86 instructions.
//!
//! Those instructions provided by the `x86` crate as `unsafe` functions, due to
//! the fact that those require certain preconditions. The wrappers provided by
//! this module encapsulate those `unsafe`-ness since this project always
//! satisfies the preconditions and safe to call them at any context.

use core::arch::asm;
use x86::{
    controlregs::{Cr0, Cr4},
    dtables::DescriptorTablePointer,
};

/// Returns the timestamp counter value.
pub(crate) fn rdtsc() -> u64 {
    // Safety: this project runs at CPL0.
    unsafe { core::arch::x86_64::_rdtsc() }
}

/// Reads an MSR.
pub(crate) fn rdmsr(msr: u32) -> u64 {
    // Safety: this project runs at CPL0.
    unsafe { x86::msr::rdmsr(msr) }
}

/// Writes a value to an MSR.
pub(crate) fn wrmsr(msr: u32, value: u64) {
    // Safety: this project runs at CPL0.
    unsafe { x86::msr::wrmsr(msr, value) };
}

/// Reads the CR0 register.
pub(crate) fn cr0() -> Cr0 {
    // Safety: this project runs at CPL0.
    unsafe { x86::controlregs::cr0() }
}

/// Writes a value to the CR0 register.
pub(crate) fn cr0_write(val: Cr0) {
    // Safety: this project runs at CPL0.
    unsafe { x86::controlregs::cr0_write(val) };
}

/// Reads the CR3 register.
pub(crate) fn cr3() -> u64 {
    // Safety: this project runs at CPL0.
    unsafe { x86::controlregs::cr3() }
}

/// Reads the CR4 register.
pub(crate) fn cr4() -> Cr4 {
    // Safety: this project runs at CPL0.
    unsafe { x86::controlregs::cr4() }
}

/// Writes a value to the CR4 register.
pub(crate) fn cr4_write(val: Cr4) {
    // Safety: this project runs at CPL0.
    unsafe { x86::controlregs::cr4_write(val) };
}

/// Disables maskable interrupts.
pub(crate) fn cli() {
    // Safety: this project runs at CPL0.
    unsafe { x86::irq::disable() };
}

/// Halts execution of the processor.
pub(crate) fn hlt() {
    // Safety: this project runs at CPL0.
    unsafe { x86::halt() };
}

/// Reads 8-bits from an IO port.
pub(crate) fn inb(port: u16) -> u8 {
    // Safety: this project runs at CPL0.
    unsafe { x86::io::inb(port) }
}

/// Writes 8-bits to an IO port.
pub(crate) fn outb(port: u16, val: u8) {
    // Safety: this project runs at CPL0.
    unsafe { x86::io::outb(port, val) };
}

/// Reads the IDTR register.
pub(crate) fn sidt<T>(idtr: &mut DescriptorTablePointer<T>) {
    // Safety: this project runs at CPL0.
    unsafe { x86::dtables::sidt(idtr) };
}

/// Reads the GDTR.
pub(crate) fn sgdt<T>(gdtr: &mut DescriptorTablePointer<T>) {
    // Safety: this project runs at CPL0.
    unsafe { x86::dtables::sgdt(gdtr) };
}

/// Executes Bochs magic breakpoint. Noop outside Bochs.
///
/// Set "magic_break: enabled=1" in the Bochs configuration file.
// inline_always: to avoid having to step through to `RET` to the caller.
// doc_markdown: clippy confused with "magic_break".
// dead_code: ad-hoc debug support code. Normally not used.
#[allow(clippy::inline_always, clippy::doc_markdown, dead_code)]
#[inline(always)]
pub(crate) fn bochs_breakpoint() {
    unsafe { asm!("xchg %bx, %bx", options(att_syntax, nomem, nostack)) };
}
