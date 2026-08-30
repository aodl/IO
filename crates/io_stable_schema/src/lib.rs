#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StableSchemaEntry {
    pub canister_name: &'static str,
    pub current_version: u32,
    pub lossless: bool,
    pub pre_production_only: bool,
    pub fixture_files: &'static [&'static str],
    pub size_bounds_summary: &'static str,
    pub compaction_policy_summary: &'static str,
}

pub const IO_STREAM_MANAGER_SCHEMA_VERSION: u32 = 1;
pub const IO_NNS_NEURON_MANAGER_SCHEMA_VERSION: u32 = 1;
pub const IO_HISTORIAN_SCHEMA_VERSION: u32 = 1;

pub const IO_STREAM_MANAGER_FIXTURES: &[&str] =
    &["tests/fixtures/stable-state/io_stream_manager/launch-v1.fixture"];

pub const IO_NNS_NEURON_MANAGER_FIXTURES: &[&str] =
    &["tests/fixtures/stable-state/io_nns_neuron_manager/launch-v1.fixture"];

pub const IO_HISTORIAN_FIXTURES: &[&str] = &[
    "tests/fixtures/stable-state/io_historian/current.fixture",
    "tests/fixtures/stable-state/io_historian/bounded-history-near-limit.fixture",
    "tests/fixtures/stable-state/io_historian/corrupt.fixture",
    "tests/fixtures/stable-state/io_historian/future-version.fixture",
];

pub const STABLE_SCHEMA_REGISTRY: &[StableSchemaEntry] = &[
    StableSchemaEntry {
        canister_name: "io_stream_manager",
        current_version: IO_STREAM_MANAGER_SCHEMA_VERSION,
        lossless: true,
        pre_production_only: true,
        fixture_files: IO_STREAM_MANAGER_FIXTURES,
        size_bounds_summary: "one typed active operation, one bounded live entitlement accumulator, one pending batch and bounded per-caller prepared-push nonce/results",
        compaction_policy_summary: "launch V1 has no historical execution collection to compact",
    },
    StableSchemaEntry {
        canister_name: "io_nns_neuron_manager",
        current_version: IO_NNS_NEURON_MANAGER_SCHEMA_VERSION,
        lossless: true,
        pre_production_only: true,
        fixture_files: IO_NNS_NEURON_MANAGER_FIXTURES,
        size_bounds_summary: "one immediate operation, aggregate Dynamic fee scalars, semantic maturity state, and generation-based passive cohorts",
        compaction_policy_summary: "launch V1 has no historical execution collection to compact",
    },
    StableSchemaEntry {
        canister_name: "io_historian",
        current_version: IO_HISTORIAN_SCHEMA_VERSION,
        lossless: true,
        pre_production_only: true,
        fixture_files: IO_HISTORIAN_FIXTURES,
        size_bounds_summary: "typed source configuration and recent index Account histories are capped",
        compaction_policy_summary: "launch V1 keeps only bounded current observation state",
    },
];

pub fn registry_entry(canister_name: &str) -> Option<&'static StableSchemaEntry> {
    STABLE_SCHEMA_REGISTRY
        .iter()
        .find(|entry| entry.canister_name == canister_name)
}

pub fn accepts_schema_version(entry: &StableSchemaEntry, version: u32) -> bool {
    version == entry.current_version
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
    fn only_current_launch_schema_is_accepted() {
        for entry in STABLE_SCHEMA_REGISTRY {
            assert!(accepts_schema_version(entry, entry.current_version));
            assert!(!accepts_schema_version(
                entry,
                entry.current_version.saturating_sub(1)
            ));
            assert!(!accepts_schema_version(
                entry,
                entry.current_version.saturating_add(1)
            ));
        }
    }
}
