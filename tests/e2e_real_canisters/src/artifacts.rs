use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub const DEFAULT_MANIFEST: &str = "tests/e2e_real_canisters/wasms.local.toml";
pub const ENV_WASM_DIR: &str = "IO_REAL_SNS_WASM_DIR";
pub const ENV_MANIFEST: &str = "IO_REAL_SNS_WASM_MANIFEST";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArtifactStatus {
    Skipped(String),
    Ready(ArtifactSet),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactSet {
    pub wasm_dir: PathBuf,
    pub manifest_path: Option<PathBuf>,
    pub manifest: ArtifactManifest,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ArtifactManifest {
    entries: BTreeMap<String, String>,
}

impl ArtifactManifest {
    pub fn parse(input: &str) -> Result<Self, String> {
        let mut section = String::new();
        let mut entries = BTreeMap::new();
        for (line_no, raw) in input.lines().enumerate() {
            let without_comment = raw.split_once('#').map_or(raw, |(prefix, _)| prefix).trim();
            if without_comment.is_empty() {
                continue;
            }
            if without_comment.starts_with('[') && without_comment.ends_with(']') {
                section = without_comment
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .trim()
                    .to_string();
                continue;
            }
            let Some((key, value)) = without_comment.split_once('=') else {
                return Err(format!("line {}: expected key = value", line_no + 1));
            };
            let key = key.trim();
            let value = parse_quoted(value.trim())
                .ok_or_else(|| format!("line {}: value for {key} must be quoted", line_no + 1))?;
            let full_key = if section.is_empty() {
                key.to_string()
            } else {
                format!("{section}.{key}")
            };
            entries.insert(full_key, value);
        }
        Ok(Self { entries })
    }

    pub fn from_file(path: &Path) -> Result<Self, String> {
        let text = fs::read_to_string(path)
            .map_err(|err| format!("failed to read manifest {}: {err}", path.display()))?;
        Self::parse(&text)
    }

    fn field(&self, key: &str, field: &str) -> Option<&str> {
        self.entries
            .get(&format!("artifacts.{key}.{field}"))
            .or_else(|| self.entries.get(&format!("artifacts.{key}_{field}")))
            .or_else(|| self.entries.get(&format!("{key}_{field}")))
            .map(String::as_str)
    }

    pub fn artifact_name(&self, key: &str) -> Result<&str, String> {
        self.field(key, "filename")
            .or_else(|| self.field(key, "wasm"))
            .map(|value| value.trim())
            .filter(|value| !value.is_empty() && !value.starts_with('<'))
            .ok_or_else(|| format!("manifest is missing artifacts.{key}.filename"))
    }

    pub fn expected_hash(&self, key: &str) -> Option<&str> {
        self.field(key, "sha256")
            .filter(|value| !value.starts_with('<'))
    }

    pub fn require_hash(&self, key: &str) -> Result<&str, String> {
        self.expected_hash(key)
            .ok_or_else(|| format!("manifest is missing pinned artifacts.{key}_sha256"))
    }

    pub fn source_url(&self, key: &str) -> Option<&str> {
        self.field(key, "source_url")
            .filter(|value| !value.starts_with('<'))
    }

    pub fn source_sha256(&self, key: &str) -> Option<&str> {
        self.field(key, "source_sha256")
            .filter(|value| !value.starts_with('<'))
    }

    pub fn source_kind(&self, key: &str) -> Option<&str> {
        self.field(key, "source_kind")
            .filter(|value| !value.starts_with('<'))
    }

    pub fn source_filename(&self, key: &str) -> Option<&str> {
        self.field(key, "source_filename")
            .filter(|value| !value.starts_with('<'))
    }

    pub fn require_fetch_metadata(&self, key: &str) -> Result<FetchMetadata<'_>, String> {
        let source_url = self
            .source_url(key)
            .ok_or_else(|| format!("manifest is missing pinned artifacts.{key}.source_url"))?;
        let source_sha256 = self
            .source_sha256(key)
            .ok_or_else(|| format!("manifest is missing pinned artifacts.{key}.source_sha256"))?;
        let source_kind = self
            .source_kind(key)
            .ok_or_else(|| format!("manifest is missing artifacts.{key}.source_kind"))?;
        Ok(FetchMetadata {
            source_url,
            source_sha256,
            source_kind,
            source_filename: self.source_filename(key),
        })
    }

    pub fn has_artifact(&self, key: &str) -> bool {
        self.artifact_name(key).is_ok()
            && (self.expected_hash(key).is_some() || self.source_sha256(key).is_some())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FetchMetadata<'a> {
    pub source_url: &'a str,
    pub source_sha256: &'a str,
    pub source_kind: &'a str,
    pub source_filename: Option<&'a str>,
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

fn parse_quoted(value: &str) -> Option<String> {
    let value = value.trim();
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        return Some(value[1..value.len() - 1].to_string());
    }
    None
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
        let manifest = ArtifactManifest::parse(
            r#"
            sns_ledger_wasm = "sns_ledger.wasm"
            sns_ledger_sha256 = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            sns_ledger_source_kind = "dfinity_release_store"
            sns_ledger_source_url = "pinned-url"
            sns_ledger_source_sha256 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            "#,
        )
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
}
