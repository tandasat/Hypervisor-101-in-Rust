//! The module containing various constants that may be modified by developers.

/// The logging level.
pub(crate) const LOGGING_LEVEL: log::LevelFilter = log::LevelFilter::Trace;

/// Once in how many iterations stats should be sent to the serial output.
/// Ignored when [`LOGGING_LEVEL`] is `Trace`.
pub(crate) const SERIAL_OUTPUT_INTERVAL: u64 = 500;

/// Once in how many iterations stats should be displayed on the console.
/// Ignored when `stdout_stats_report` is disabled.
pub(crate) const CONSOLE_OUTPUT_INTERVAL: u64 = 1000;

/// How long a single fuzzing iteration can spend within the guest-mode, in TSC.
/// If the more than this is spent, a timer fires and aborts the VM.
pub(crate) const GUEST_EXEC_TIMEOUT_IN_TSC: u64 = 200_000_000;

/// The number of fuzzing iterations to be done for single input. The lower, the
/// more frequently new files are selected, and it is slightly costly. Ignored
/// when `random_byte_modification` is disabled.
pub(crate) const MAX_ITERATION_COUNT_PER_FILE: u64 = 10_000;
