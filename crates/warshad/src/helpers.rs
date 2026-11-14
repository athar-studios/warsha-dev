//! Shared leaf helpers: the preview run-liveness oracle (R-BRIDGE-B5 D3) and the constants
//! more than one daemon module reads. P-H1 A0 — moved verbatim out of `lib.rs`.

use jsonrpsee::core::async_trait;
use std::sync::Arc;
use warsha_core::{RunId, RunStatus};
use warsha_store::Store;

/// The store's answer to "is this run still live", for the preview manager.
///
/// `warsha-pipeline` owns the preview entries and knows nothing about run records,
/// so the daemon hands it this oracle (R-BRIDGE-B5 D3). A run the store still has
/// and that has not finished is live, and its preview is not another run's to take.
/// A run the store no longer has, an id that is not a run id, or a store read that
/// fails is *not* live: a preview entry whose owner cannot be proved alive is
/// swept, which is what keeps a new run on the project from inheriting a server a
/// dead run left behind.
pub(crate) struct StoreRunLiveness {
    pub(crate) store: Arc<Store>,
}

#[async_trait]
impl warsha_pipeline::RunLiveness for StoreRunLiveness {
    async fn is_live(&self, run_id: &str) -> bool {
        let Ok(id) = run_id.parse::<ulid::Ulid>() else {
            return false;
        };
        match self.store.get_run(RunId(id)).await {
            Ok(run) => !matches!(
                run.status,
                RunStatus::Completed | RunStatus::Failed | RunStatus::Cancelled
            ),
            Err(_) => false,
        }
    }
}

/// Directory under the daemon's data directory that holds handoff bundles.
pub(crate) const HANDOFF_DIR: &str = "handoff";

/// The selection key a harness carries when no project was named.
///
/// `harnesses_active` is keyed by a text id and every other caller in this module uses
/// a project's UUID; `global` is the one key that cannot collide with a UUID, so the
/// daemon-wide default and a project's override live in the same table without either
/// pretending to be the other. A project id that a caller *does* pass is used verbatim
/// as its key, which is what `harness.select` from a browser and a run's own project
/// both read.
pub(crate) const ACTIVE_HARNESS_GLOBAL_KEY: &str = "global";

/// The local PocketBase address PLAN.md M19 documents for a generated project.
pub(crate) const LOCAL_POCKETBASE_URL: &str = "http://127.0.0.1:8090";
