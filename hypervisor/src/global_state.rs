//! The module containing the [`GlobalState`] type.

use crate::{
    corpus::Corpus,
    patch::PatchSet,
    snapshot::Snapshot,
    stats::{time, time_to_u64, RunStats},
    system_table::system_table_unsafe,
};
use core::sync::atomic::{AtomicU64, Ordering};
use spin::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use uefi::{
    proto::pi::mp::MpServices,
    table::boot::{OpenProtocolAttributes, OpenProtocolParams},
};

/// The singleton data structure that is used across all processors. Any write
/// access to this structure must be synchronized.
pub(crate) struct GlobalState {
    /// The number of logical processors currently performing fuzzing.
    // Incremented when a logical processor starts fuzzing. Decremented when it
    // waits for new input file. If this becomes zero, fuzzing is complete and
    // the hypervisor panics.
    pub(crate) active_thread_count: AtomicU64,
    snapshot: RwLock<Snapshot>,
    corpus: Corpus,
    overall_stats: RwLock<RunStats>,
    patch_set: PatchSet,
    iteration_count: AtomicU64,
    number_of_cores: u64,
    start_time: u64,
}

impl GlobalState {
    pub(crate) fn new(
        snapshot_path: &str,
        patch_path: &str,
        corpus_path: &str,
    ) -> Result<Self, uefi::Error> {
        // Safety: Code is single threaded.
        let st = unsafe { system_table_unsafe() };
        let bs = st.boot_services();
        let mp = unsafe {
            bs.open_protocol::<MpServices>(
                OpenProtocolParams {
                    handle: bs.get_handle_for_protocol::<MpServices>()?,
                    agent: bs.image_handle(),
                    controller: None,
                },
                OpenProtocolAttributes::GetProtocol,
            )?
        };
        let mut dir = bs.get_image_file_system(bs.image_handle())?.open_volume()?;
        let snapshot = Snapshot::new(&mut dir, snapshot_path)?;
        let corpus = Corpus::new(&mut dir, corpus_path, &snapshot)?;
        Ok(Self {
            active_thread_count: AtomicU64::new(0),
            snapshot: RwLock::new(snapshot),
            corpus,
            overall_stats: RwLock::new(RunStats::new()),
            patch_set: PatchSet::new(&mut dir, patch_path)?,
            iteration_count: AtomicU64::new(0),
            number_of_cores: mp.get_number_of_processors()?.enabled as u64,
            start_time: time_to_u64(time()),
        })
    }

    pub(crate) fn snapshot(&self) -> RwLockReadGuard<'_, Snapshot> {
        self.snapshot.read()
    }

    pub(crate) fn snapshot_mut(&self) -> RwLockWriteGuard<'_, Snapshot> {
        self.snapshot.write()
    }

    pub(crate) fn corpus(&self) -> &Corpus {
        &self.corpus
    }

    pub(crate) fn clone_stats(&self) -> RunStats {
        self.overall_stats.read().clone()
    }

    pub(crate) fn patch_set(&self) -> &PatchSet {
        &self.patch_set
    }

    pub(crate) fn number_of_cores(&self) -> u64 {
        self.number_of_cores
    }

    pub(crate) fn iter_count(&self) -> u64 {
        self.iteration_count.load(Ordering::SeqCst)
    }

    pub(crate) fn start_time(&self) -> u64 {
        self.start_time
    }

    /// Updates the overall statistics with the new statistics `stats`.
    pub(crate) fn update_stats(&self, stats: &RunStats) -> u64 {
        let mut total_stats = self.overall_stats.write();
        total_stats.total_tsc += stats.total_tsc;
        total_stats.host_spent_tsc += stats.host_spent_tsc;
        total_stats.vmexit_count += stats.vmexit_count;
        total_stats
            .newly_executed_basic_blks
            .extend(&stats.newly_executed_basic_blks);
        total_stats.hang_count += stats.hang_count;
        self.iteration_count.fetch_add(1, Ordering::SeqCst) + 1
    }
}
