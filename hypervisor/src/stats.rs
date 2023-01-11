//! The module containing the [`RunStats`] type.

use crate::{
    config::{CONSOLE_OUTPUT_INTERVAL, SERIAL_OUTPUT_INTERVAL},
    global_state::GlobalState,
    system_table::system_table,
    x86_instructions::rdtsc,
};
use alloc::{format, vec::Vec};
use core::{fmt::Write, sync::atomic::Ordering};
use log::info;
use uefi::table::runtime::Time;

/// Statistics of one or overall fuzzing iteration.
#[derive(Default, Clone)]
pub(crate) struct RunStats {
    /// The time when the fuzzing started.
    pub(crate) start_tsc: u64,
    /// The total elapsed time in TSC.
    pub(crate) total_tsc: u64,
    /// The elapsed time spent in the host in TSC.
    pub(crate) host_spent_tsc: u64,
    /// The number of VM exit occurred.
    pub(crate) vmexit_count: u64,
    /// The number of basic blocks that are newly executed.
    pub(crate) newly_executed_basic_blks: Vec<u64>,
    /// The number of iteration that ended with hang.
    pub(crate) hang_count: u64,
}

impl RunStats {
    pub(crate) fn new() -> Self {
        Self {
            start_tsc: rdtsc(),
            ..Default::default()
        }
    }

    /// Updates the statistics, and if needed, prints them out.
    pub(crate) fn report(
        &self,
        global: &GlobalState,
        used_dirty_page_count: usize,
        iter_count: u64,
    ) {
        if iter_count == 1 {
            if !cfg!(feature = "stdout_stats_report") {
                system_table().stdout().clear().unwrap();
                writeln!(
                    system_table().stdout(),
                    "Console output disabled. Enable the `stdout_stats_report` feature if desired."
                )
                .unwrap();
            }
            info!("HH:MM:SS,     Run#, Dirty Page#, New BB#, Total TSC, Guest TSC, VM exit#,");
        }

        // Serial output.
        if log::log_enabled!(log::Level::Trace)
            || !self.newly_executed_basic_blks.is_empty()
            || (iter_count % SERIAL_OUTPUT_INTERVAL) == 0
        {
            let time = time();
            info!(
                "{:02}:{:02}:{:02}, {:>8}, {:>11}, {:>7}, {:>9}, {:>9}, {:>8},",
                time.hour(),
                time.minute(),
                time.second(),
                iter_count,
                used_dirty_page_count,
                self.newly_executed_basic_blks.len(),
                self.total_tsc,
                self.total_tsc - self.host_spent_tsc,
                self.vmexit_count,
            );
            if !self.newly_executed_basic_blks.is_empty() {
                info!("COVERAGE: {:x?}", self.newly_executed_basic_blks);
            }
        }

        // Stdout output.
        if cfg!(feature = "stdout_stats_report")
            && (iter_count == 1 || (iter_count % CONSOLE_OUTPUT_INTERVAL) == 0)
        {
            Self::stdout(global, iter_count);
        }
    }

    // Prints out current statistics to the console.
    fn stdout(global: &GlobalState, iter_count: u64) {
        let global_stats = global.clone_stats();
        let time = time();
        let time_u64 = time_to_u64(time);
        let elapsed_seconds = if time_u64 > global.start_time() {
            time_u64 - global.start_time()
        } else {
            1
        };
        let text = format!(
            "
                        Last update: {:02}:{:02}:{:02}
                    Total Iteration: {}
        Total executed basic blocks: {}
                   Total hang count: {}
             Remaining corpus files: {}
                Active thread count: {}
              Average VM exit count: {}
 Average iteration count per second: {}
Average overall cycle per iteration: {}
  Average guest cycle per iteration: {}
",
            time.hour(),
            time.minute(),
            time.second(),
            iter_count,
            global_stats.newly_executed_basic_blks.len(),
            global_stats.hang_count,
            global.corpus().remaining_files_count(),
            global.active_thread_count.load(Ordering::SeqCst),
            global_stats.vmexit_count / iter_count,
            iter_count / elapsed_seconds,
            global_stats.total_tsc / iter_count,
            (global_stats.total_tsc - global_stats.host_spent_tsc) / iter_count,
        );
        system_table().stdout().clear().unwrap();
        write!(system_table().stdout(), "{text}").unwrap();
    }
}

/// Returns the current time if `time_report` is enabled. Otherwise, an invalid
/// time.
pub(crate) fn time() -> Time {
    if cfg!(feature = "time_report") {
        system_table().runtime_services().get_time().unwrap()
    } else {
        Time::invalid()
    }
}

/// Converts the [`Time`] represented time to [`u64`].
pub(crate) fn time_to_u64(time: Time) -> u64 {
    u64::from(time.hour()) * 3600 + u64::from(time.minute()) * 60 + u64::from(time.second())
}
