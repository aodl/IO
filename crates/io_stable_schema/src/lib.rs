#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StableSchemaEntry {
    pub canister_name: &'static str,
    pub current_version: u32,
    pub previous_supported_versions: &'static [u32],
    pub migration_paths: &'static [&'static str],
    pub lossless: bool,
    pub pre_production_only: bool,
    pub fixture_files: &'static [&'static str],
    pub size_bounds_summary: &'static str,
    pub compaction_policy_summary: &'static str,
}

pub const IO_STREAM_MANAGER_SCHEMA_VERSION: u32 = 1;
pub const IO_NNS_NEURON_MANAGER_SCHEMA_VERSION: u32 = 1;
pub const IO_HISTORIAN_SCHEMA_VERSION: u32 = 1;

pub const IO_STREAM_MANAGER_FIXTURES: &[&str] = &[
    "tests/fixtures/stable-state/io_stream_manager/current.fixture",
    "tests/fixtures/stable-state/io_stream_manager/previous-minimal.fixture",
    "tests/fixtures/stable-state/io_stream_manager/missing-optional-fields.fixture",
    "tests/fixtures/stable-state/io_stream_manager/empty-default.fixture",
    "tests/fixtures/stable-state/io_stream_manager/pending-redemption-journal.fixture",
    "tests/fixtures/stable-state/io_stream_manager/corrupt.fixture",
    "tests/fixtures/stable-state/io_stream_manager/future-version.fixture",
];

pub const IO_NNS_NEURON_MANAGER_FIXTURES: &[&str] = &[
    "tests/fixtures/stable-state/io_nns_neuron_manager/current.fixture",
    "tests/fixtures/stable-state/io_nns_neuron_manager/previous-minimal.fixture",
    "tests/fixtures/stable-state/io_nns_neuron_manager/missing-optional-fields.fixture",
    "tests/fixtures/stable-state/io_nns_neuron_manager/empty-default.fixture",
    "tests/fixtures/stable-state/io_nns_neuron_manager/pending-lifecycle-journal.fixture",
    "tests/fixtures/stable-state/io_nns_neuron_manager/corrupt.fixture",
    "tests/fixtures/stable-state/io_nns_neuron_manager/future-version.fixture",
];

pub const IO_HISTORIAN_FIXTURES: &[&str] = &[
    "tests/fixtures/stable-state/io_historian/current.fixture",
    "tests/fixtures/stable-state/io_historian/previous-minimal.fixture",
    "tests/fixtures/stable-state/io_historian/missing-source-health.fixture",
    "tests/fixtures/stable-state/io_historian/empty-default.fixture",
    "tests/fixtures/stable-state/io_historian/bounded-history-near-limit.fixture",
    "tests/fixtures/stable-state/io_historian/corrupt.fixture",
    "tests/fixtures/stable-state/io_historian/future-version.fixture",
];

pub const STABLE_SCHEMA_REGISTRY: &[StableSchemaEntry] = &[
    StableSchemaEntry {
        canister_name: "io_stream_manager",
        current_version: IO_STREAM_MANAGER_SCHEMA_VERSION,
        previous_supported_versions: &[0],
        migration_paths: &["v0_unversioned_snapshot_to_v1_envelope"],
        lossless: true,
        pre_production_only: true,
        fixture_files: IO_STREAM_MANAGER_FIXTURES,
        size_bounds_summary: "retry-critical operation journal and processed transaction set are not silently evicted; account-history cursors are scalar bounded state",
        compaction_policy_summary: "no automatic compaction before audit/activation; completed and duplicate-proof records need explicit safe checkpoints",
    },
    StableSchemaEntry {
        canister_name: "io_nns_neuron_manager",
        current_version: IO_NNS_NEURON_MANAGER_SCHEMA_VERSION,
        previous_supported_versions: &[0],
        migration_paths: &["v0_unversioned_snapshot_to_v1_envelope"],
        lossless: true,
        pre_production_only: true,
        fixture_files: IO_NNS_NEURON_MANAGER_FIXTURES,
        size_bounds_summary: "retry-critical lifecycle journal is not silently evicted; scheduler cursors are scalar bounded state",
        compaction_policy_summary: "no automatic compaction before audit/activation; terminal lifecycle records need explicit audit checkpoints",
    },
    StableSchemaEntry {
        canister_name: "io_historian",
        current_version: IO_HISTORIAN_SCHEMA_VERSION,
        previous_supported_versions: &[0],
        migration_paths: &["v0_missing_source_health_to_v1_recomputed_read_model"],
        lossless: true,
        pre_production_only: false,
        fixture_files: IO_HISTORIAN_FIXTURES,
        size_bounds_summary: "read-model histories are bounded and rebuildable; page limits are capped",
        compaction_policy_summary: "bounded read-model eviction keeps newest deterministic records and loses only rebuildable convenience data",
    },
];

pub fn registry_entry(canister_name: &str) -> Option<&'static StableSchemaEntry> {
    STABLE_SCHEMA_REGISTRY
        .iter()
        .find(|entry| entry.canister_name == canister_name)
}

pub fn accepts_schema_version(entry: &StableSchemaEntry, version: u32) -> bool {
    version == entry.current_version || entry.previous_supported_versions.contains(&version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_required_canisters_are_registered() {
        for name in ["io_stream_manager", "io_nns_neuron_manager", "io_historian"] {
            let entry = registry_entry(name).expect("registered canister");
            assert_ne!(entry.current_version, 0);
            assert!(!entry.fixture_files.is_empty());
        }
    }

    #[test]
    fn previous_schema_versions_are_ordered_and_future_rejects() {
        for entry in STABLE_SCHEMA_REGISTRY {
            assert!(
                entry
                    .previous_supported_versions
                    .windows(2)
                    .all(|pair| pair[0] < pair[1]),
                "{} previous versions must be sorted",
                entry.canister_name
            );
            assert!(accepts_schema_version(entry, entry.current_version));
            assert!(!accepts_schema_version(
                entry,
                entry.current_version.saturating_add(1)
            ));
        }
    }
}
