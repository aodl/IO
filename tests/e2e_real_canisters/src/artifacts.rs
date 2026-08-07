use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub use io_sns_manifest::{FetchMetadata, SnsManifest as ArtifactManifest};

pub const DEFAULT_MANIFEST: &str = "tests/e2e_real_canisters/wasms.local.toml";
pub const ENV_WASM_DIR: &str = "IO_REAL_SNS_WASM_DIR";
pub const ENV_MANIFEST: &str = "IO_REAL_SNS_WASM_MANIFEST";

#[derive(Clone, Debug, PartialEq)]
pub enum ArtifactStatus {
    Skipped(String),
    Ready(ArtifactSet),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ArtifactSet {
    pub wasm_dir: PathBuf,
    pub manifest_path: Option<PathBuf>,
    pub manifest: ArtifactManifest,
}

impl ArtifactSet {
    pub fn load_required(&self, key: &str) -> Result<Vec<u8>, String> {
        let file_name = self.manifest.artifact_name(key)?;
        let path = self.wasm_dir.join(file_name);
        let bytes = fs::read(&path)
            .map_err(|err| format!("failed to read artifact {}: {err}", path.display()))?;
        let expected = self.manifest.require_hash(key)?;
        verify_sha256_bytes(&path, &bytes, expected)?;
        Ok(bytes)
    }

    pub fn load_required_source_wasm_gz(&self, key: &str) -> Result<Vec<u8>, String> {
        let file_name = self.manifest.source_artifact_name(key)?;
        if !file_name.ends_with(".wasm.gz") && !file_name.ends_with(".gz") {
            return Err(format!(
                "artifact {key} source file {file_name} is not a .wasm.gz artifact"
            ));
        }
        let path = self.wasm_dir.join(file_name);
        let bytes = fs::read(&path).map_err(|err| {
            format!(
                "failed to read source artifact {}: {err}; run tools/scripts/fetch-real-canister-artifacts",
                path.display()
            )
        })?;
        let expected = self
            .manifest
            .source_sha256(key)
            .ok_or_else(|| format!("manifest is missing pinned artifacts.{key}.source_sha256"))?;
        verify_sha256_bytes(&path, &bytes, expected)?;
        Ok(bytes)
    }
}

pub fn resolve_from_env(required: bool) -> Result<ArtifactStatus, String> {
    let Some(wasm_dir) = env::var_os(ENV_WASM_DIR).map(PathBuf::from) else {
        if required {
            return Err(format!(
                "{ENV_WASM_DIR} is required for this real-canister gate"
            ));
        }
        return Ok(ArtifactStatus::Skipped(format!(
            "set {ENV_WASM_DIR} to run real-framework PocketIC tests"
        )));
    };
    if !wasm_dir.is_dir() {
        return Err(format!(
            "{ENV_WASM_DIR} must point to an existing directory: {}",
            wasm_dir.display()
        ));
    }

    let manifest_path = env::var_os(ENV_MANIFEST).map(PathBuf::from).or_else(|| {
        Path::new(DEFAULT_MANIFEST)
            .is_file()
            .then(|| PathBuf::from(DEFAULT_MANIFEST))
    });
    let manifest = match &manifest_path {
        Some(path) => ArtifactManifest::from_file(path)?,
        None if required => {
            return Err(format!(
                "{ENV_MANIFEST} or {DEFAULT_MANIFEST} is required for this real-canister gate"
            ));
        }
        None => {
            return Ok(ArtifactStatus::Skipped(format!(
                "set {ENV_MANIFEST} or create {DEFAULT_MANIFEST} with pinned SHA-256 values"
            )));
        }
    };

    Ok(ArtifactStatus::Ready(ArtifactSet {
        wasm_dir,
        manifest_path,
        manifest,
    }))
}

pub fn verify_sha256_bytes(path: &Path, bytes: &[u8], expected_hex: &str) -> Result<(), String> {
    let expected = expected_hex.trim().to_ascii_lowercase();
    if expected.len() != 64 || !expected.as_bytes().iter().all(u8::is_ascii_hexdigit) {
        return Err(format!(
            "{}: expected SHA-256 must be 64 lowercase/uppercase hex characters",
            path.display()
        ));
    }
    let actual = hex::encode(Sha256::digest(bytes));
    if actual != expected {
        return Err(format!(
            "{}: SHA-256 mismatch; expected {expected}, got {actual}",
            path.display()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn clear_env() {
        env::remove_var(ENV_WASM_DIR);
        env::remove_var(ENV_MANIFEST);
    }

    #[test]
    fn manifest_parsing_reads_required_artifacts() {
        let manifest = ArtifactManifest::parse(
            r#"
            [artifacts]
            sns_ledger_wasm = "sns_ledger.wasm"
            sns_ledger_sha256 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            sns_index_wasm = "sns_index.wasm"
            sns_index_sha256 = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            "#,
        )
        .unwrap();
        assert_eq!(
            manifest.artifact_name("sns_ledger").unwrap(),
            "sns_ledger.wasm"
        );
        assert_eq!(
            manifest.require_hash("sns_index").unwrap(),
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        );
    }

    #[test]
    fn nested_manifest_parsing_reads_required_artifacts() {
        let manifest = ArtifactManifest::parse(
            r#"
            [artifacts.sns_ledger]
            filename = "sns_ledger.wasm"
            source_filename = "sns_ledger.wasm.gz"
            source_kind = "dfinity_release_store"
            source_url = "pinned-url"
            source_sha256 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            sha256 = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            upstream_rev = "rev"
            license = "Apache-2.0"
            "#,
        )
        .unwrap();
        assert_eq!(
            manifest.artifact_name("sns_ledger").unwrap(),
            "sns_ledger.wasm"
        );
        assert_eq!(
            manifest.source_filename("sns_ledger"),
            Some("sns_ledger.wasm.gz")
        );
        assert_eq!(
            manifest.source_artifact_name("sns_ledger").unwrap(),
            "sns_ledger.wasm.gz"
        );
        assert_eq!(
            manifest.require_hash("sns_ledger").unwrap(),
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        );
        assert_eq!(
            manifest.require_fetch_metadata("sns_ledger").unwrap(),
            FetchMetadata {
                source_url: "pinned-url",
                source_sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                source_kind: "dfinity_release_store",
                source_filename: Some("sns_ledger.wasm.gz"),
            }
        );
    }

    #[test]
    fn legacy_flat_manifest_parsing_is_preserved() {
        let source_url = concat!(
            "https",
            "://",
            "down",
            "load.dfinity.systems/ic/rev/canisters/ic-icrc1-ledger.wasm.gz"
        );
        let manifest = ArtifactManifest::parse(&format!(
            r#"
            sns_ledger_wasm = "sns_ledger.wasm"
            sns_ledger_sha256 = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            sns_ledger_source_kind = "dfinity_release_store"
            sns_ledger_source_url = "{source_url}"
            sns_ledger_source_sha256 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            "#
        ))
        .unwrap();
        assert_eq!(
            manifest.artifact_name("sns_ledger").unwrap(),
            "sns_ledger.wasm"
        );
        assert_eq!(
            manifest
                .require_fetch_metadata("sns_ledger")
                .unwrap()
                .source_sha256,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
        assert_eq!(
            manifest.source_artifact_name("sns_ledger").unwrap(),
            "ic-icrc1-ledger.wasm.gz"
        );
    }

    #[test]
    fn missing_source_metadata_is_error_for_fetch() {
        let manifest = ArtifactManifest::parse(
            r#"
            [artifacts.sns_ledger]
            filename = "sns_ledger.wasm"
            sha256 = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            "#,
        )
        .unwrap();
        let err = manifest.require_fetch_metadata("sns_ledger").unwrap_err();
        assert!(err.contains("source_url"));
    }

    #[test]
    fn env_absent_means_opt_in_skip() {
        let _guard = crate::lock_test_env();
        clear_env();
        match resolve_from_env(false).unwrap() {
            ArtifactStatus::Skipped(message) => assert!(message.contains(ENV_WASM_DIR)),
            ArtifactStatus::Ready(_) => panic!("expected skip when env is absent"),
        }
    }

    #[test]
    fn required_env_absent_is_error() {
        let _guard = crate::lock_test_env();
        clear_env();
        assert!(resolve_from_env(true).unwrap_err().contains(ENV_WASM_DIR));
    }

    #[test]
    fn source_hash_mismatch_is_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("artifact.wasm.gz");
        fs::write(&path, b"compressed bytes").unwrap();
        let err = verify_sha256_bytes(
            &path,
            &fs::read(&path).unwrap(),
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        )
        .unwrap_err();
        assert!(err.contains("SHA-256 mismatch"));
    }

    #[test]
    fn decompressed_hash_mismatch_is_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("artifact.wasm");
        fs::write(&path, b"not this hash").unwrap();
        let err = verify_sha256_bytes(
            &path,
            &fs::read(&path).unwrap(),
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        )
        .unwrap_err();
        assert!(err.contains("SHA-256 mismatch"));
    }

    #[test]
    fn required_manifest_missing_is_error() {
        let _guard = crate::lock_test_env();
        clear_env();
        let dir = tempfile::tempdir().unwrap();
        env::set_var(ENV_WASM_DIR, dir.path());
        let err = resolve_from_env(true).unwrap_err();
        assert!(err.contains(ENV_MANIFEST));
        clear_env();
    }

    #[test]
    fn configured_artifacts_are_verified() {
        let _guard = crate::lock_test_env();
        clear_env();
        let dir = tempfile::tempdir().unwrap();
        let manifest_path = dir.path().join("wasms.local.toml");
        let wasm_path = dir.path().join("sns_ledger.wasm");
        fs::write(&wasm_path, b"ledger").unwrap();
        let hash = hex::encode(Sha256::digest(b"ledger"));
        let source_url = [
            "https",
            "://down",
            "load.dfinity.systems/ic/rev/canisters/ic-icrc1-ledger.wasm.gz",
        ]
        .concat();
        fs::write(
            &manifest_path,
            format!(
                r#"[artifacts.sns_ledger]
filename = "sns_ledger.wasm"
sha256 = "{hash}"
source_kind = "dfinity_release_store"
source_url = "{source_url}"
source_sha256 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
"#
            ),
        )
        .unwrap();
        env::set_var(ENV_WASM_DIR, dir.path());
        env::set_var(ENV_MANIFEST, &manifest_path);
        let ArtifactStatus::Ready(set) = resolve_from_env(true).unwrap() else {
            panic!("expected configured artifact set");
        };
        assert_eq!(set.load_required("sns_ledger").unwrap(), b"ledger");
        assert_eq!(
            set.manifest.source_kind("sns_ledger"),
            Some("dfinity_release_store")
        );
        assert_eq!(
            set.manifest.source_sha256("sns_ledger"),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
        clear_env();
    }

    #[test]
    fn configured_source_artifacts_are_verified_from_explicit_source_filename() {
        let _guard = crate::lock_test_env();
        clear_env();
        let dir = tempfile::tempdir().unwrap();
        let manifest_path = dir.path().join("wasms.local.toml");
        fs::write(dir.path().join("sns_ledger.wasm"), b"ledger").unwrap();
        fs::write(dir.path().join("sns_ledger.wasm.gz"), b"compressed ledger").unwrap();
        let wasm_hash = hex::encode(Sha256::digest(b"ledger"));
        let source_hash = hex::encode(Sha256::digest(b"compressed ledger"));
        let source_url = concat!(
            "https",
            "://",
            "down",
            "load.dfinity.systems/ic/rev/canisters/ic-icrc1-ledger.wasm.gz"
        );
        fs::write(
            &manifest_path,
            format!(
                r#"[artifacts.sns_ledger]
filename = "sns_ledger.wasm"
sha256 = "{wasm_hash}"
source_filename = "sns_ledger.wasm.gz"
source_kind = "dfinity_release_store"
source_url = "{source_url}"
source_sha256 = "{source_hash}"
"#
            ),
        )
        .unwrap();
        env::set_var(ENV_WASM_DIR, dir.path());
        env::set_var(ENV_MANIFEST, &manifest_path);
        let ArtifactStatus::Ready(set) = resolve_from_env(true).unwrap() else {
            panic!("expected configured artifact set");
        };
        assert_eq!(
            set.load_required_source_wasm_gz("sns_ledger").unwrap(),
            b"compressed ledger"
        );
        clear_env();
    }

    #[test]
    fn configured_source_artifacts_are_verified_from_source_url_filename() {
        let _guard = crate::lock_test_env();
        clear_env();
        let dir = tempfile::tempdir().unwrap();
        let manifest_path = dir.path().join("wasms.local.toml");
        fs::write(dir.path().join("sns_ledger.wasm"), b"ledger").unwrap();
        fs::write(
            dir.path().join("ic-icrc1-ledger.wasm.gz"),
            b"compressed ledger",
        )
        .unwrap();
        let wasm_hash = hex::encode(Sha256::digest(b"ledger"));
        let source_hash = hex::encode(Sha256::digest(b"compressed ledger"));
        let source_url = concat!(
            "https",
            "://",
            "down",
            "load.dfinity.systems/ic/rev/canisters/ic-icrc1-ledger.wasm.gz"
        );
        fs::write(
            &manifest_path,
            format!(
                r#"[artifacts.sns_ledger]
filename = "sns_ledger.wasm"
sha256 = "{wasm_hash}"
source_kind = "dfinity_release_store"
source_url = "{source_url}"
source_sha256 = "{source_hash}"
"#
            ),
        )
        .unwrap();
        env::set_var(ENV_WASM_DIR, dir.path());
        env::set_var(ENV_MANIFEST, &manifest_path);
        let ArtifactStatus::Ready(set) = resolve_from_env(true).unwrap() else {
            panic!("expected configured artifact set");
        };
        assert_eq!(
            set.load_required_source_wasm_gz("sns_ledger").unwrap(),
            b"compressed ledger"
        );
        clear_env();
    }
}
