//! The module containing the UART (serial port) logger implementation.

// Inspired by Ian Kronquist's work.
// https://github.com/iankronquist/rustyvisor/blob/83b53ac104d85073858ba83326a28a6e08d1af12/pcuart/src/lib.rs

use crate::{
    config::LOGGING_LEVEL,
    x86_instructions::{inb, outb},
};
use core::{fmt, fmt::Write};
use spin::Mutex;

/// Initializes the logger instance.
pub(crate) fn init_uart_logger() {
    log::set_logger(&UART_LOGGER)
        .map(|()| log::set_max_level(LOGGING_LEVEL))
        .unwrap();
}

#[derive(Clone, Copy)]
#[repr(u16)]
enum UartComPort {
    Com1 = 0x3f8,
}

#[derive(Default)]
struct Uart {
    io_port_base: u16,
}

impl Uart {
    const fn new(port: UartComPort) -> Self {
        Self {
            io_port_base: port as u16,
        }
    }
}

const UART_OFFSET_TRANSMITTER_HOLDING_BUFFER: u16 = 0;
const UART_OFFSET_LINE_STATUS: u16 = 5;

impl Write for Uart {
    // Writes bytes `string` to the serial port.
    fn write_str(&mut self, string: &str) -> Result<(), fmt::Error> {
        for byte in string.bytes() {
            while (inb(self.io_port_base + UART_OFFSET_LINE_STATUS) & 0x20) == 0 {}
            outb(self.io_port_base + UART_OFFSET_TRANSMITTER_HOLDING_BUFFER, byte);
        }
        Ok(())
    }
}

struct UartLogger {
    port: Mutex<Uart>,
}
impl UartLogger {
    const fn new(port: UartComPort) -> Self {
        Self {
            port: Mutex::new(Uart::new(port)),
        }
    }

    fn lock(&self) -> spin::MutexGuard<'_, Uart> {
        self.port.lock()
    }
}
impl log::Log for UartLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Trace
    }

    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            let _ = writeln!(self.lock(), "#{}:{}: {}", apic_id(), record.level(), record.args());
        }
    }

    fn flush(&self) {}
}

/// Gets an APIC ID.
fn apic_id() -> u32 {
    // See: (AMD) CPUID Fn0000_0001_EBX LocalApicId, LogicalProcessorCount, CLFlush
    // See: (Intel) Table 3-8. Information Returned by CPUID Instruction
    x86::cpuid::cpuid!(0x1).ebx >> 24
}

static UART_LOGGER: UartLogger = UartLogger::new(UartComPort::Com1);
