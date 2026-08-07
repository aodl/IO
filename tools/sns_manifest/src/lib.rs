use std::fs;
use std::path::Path;

#[derive(Clone, Debug, PartialEq)]
pub struct SnsManifest {
    document: toml::Value,
}

impl SnsManifest {
    pub fn parse(text: &str) -> Result<Self, String> {
        let document = text
            .parse::<toml::Value>()
            .map_err(|error| format!("invalid TOML manifest: {error}"))?;
        if !document.is_table() {
            return Err("manifest root must be a TOML table".into());
        }
        Ok(Self { document })
    }

    pub fn read(path: &Path) -> Result<Self, String> {
        let text = fs::read_to_string(path)
            .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
        Self::parse(&text).map_err(|error| format!("{}: {error}", path.display()))
    }

    pub fn to_toml(&self) -> Result<String, String> {
        toml::to_string_pretty(&self.document)
            .map_err(|error| format!("failed to serialize TOML manifest: {error}"))
    }

    pub fn value(&self, section: &str, key: &str) -> Option<&str> {
        self.document.get(section)?.as_table()?.get(key)?.as_str()
    }

    pub fn bool_value(&self, section: &str, key: &str) -> Option<bool> {
        let value = self.document.get(section)?.as_table()?.get(key)?;
        value
            .as_bool()
            .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
    }

    pub fn artifact(&self, component: &str, field: &str) -> Option<&str> {
        let nested_field = if field == "wasm" { "filename" } else { field };
        let artifacts = self
            .document
            .get("artifacts")
            .and_then(toml::Value::as_table);
        let nested = artifacts
            .and_then(|artifacts| artifacts.get(component))
            .and_then(toml::Value::as_table)
            .and_then(|table| table.get(nested_field))
            .and_then(toml::Value::as_str);
        if nested.is_some() {
            return nested;
        }
        let legacy_field = if matches!(field, "filename" | "wasm") {
            format!("{component}_wasm")
        } else {
            format!("{component}_{field}")
        };
        artifacts
            .and_then(|artifacts| artifacts.get(&legacy_field))
            .or_else(|| self.document.get(&legacy_field))
            .and_then(toml::Value::as_str)
    }

    pub fn from_file(path: &Path) -> Result<Self, String> {
        Self::read(path)
    }

    pub fn artifact_name(&self, component: &str) -> Result<&str, String> {
        self.artifact(component, "filename")
            .map(str::trim)
            .filter(|value| !value.is_empty() && !value.starts_with('<'))
            .ok_or_else(|| format!("manifest is missing artifacts.{component}.filename"))
    }

    pub fn expected_hash(&self, component: &str) -> Option<&str> {
        self.artifact(component, "sha256")
            .filter(|value| !value.starts_with('<'))
    }

    pub fn require_hash(&self, component: &str) -> Result<&str, String> {
        self.expected_hash(component)
            .ok_or_else(|| format!("manifest is missing pinned artifacts.{component}_sha256"))
    }

    pub fn source_url(&self, component: &str) -> Option<&str> {
        self.artifact(component, "source_url")
            .filter(|value| !value.starts_with('<'))
    }

    pub fn source_sha256(&self, component: &str) -> Option<&str> {
        self.artifact(component, "source_sha256")
            .filter(|value| !value.starts_with('<'))
    }

    pub fn source_kind(&self, component: &str) -> Option<&str> {
        self.artifact(component, "source_kind")
            .filter(|value| !value.starts_with('<'))
    }

    pub fn source_filename(&self, component: &str) -> Option<&str> {
        self.artifact(component, "source_filename")
            .filter(|value| !value.starts_with('<'))
    }

    pub fn source_artifact_name(&self, component: &str) -> Result<String, String> {
        if let Some(source_filename) = self.source_filename(component) {
            return Ok(source_filename.trim().to_string());
        }
        let source_url = self.source_url(component).ok_or_else(|| {
            format!("manifest is missing pinned artifacts.{component}.source_url")
        })?;
        source_url
            .rsplit('/')
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty() && !value.starts_with('<'))
            .map(str::to_string)
            .ok_or_else(|| format!("manifest artifacts.{component}.source_url has no filename"))
    }

    pub fn require_fetch_metadata(&self, component: &str) -> Result<FetchMetadata<'_>, String> {
        Ok(FetchMetadata {
            source_url: self.source_url(component).ok_or_else(|| {
                format!("manifest is missing pinned artifacts.{component}.source_url")
            })?,
            source_sha256: self.source_sha256(component).ok_or_else(|| {
                format!("manifest is missing pinned artifacts.{component}.source_sha256")
            })?,
            source_kind: self
                .source_kind(component)
                .ok_or_else(|| format!("manifest is missing artifacts.{component}.source_kind"))?,
            source_filename: self.source_filename(component),
        })
    }

    pub fn has_artifact(&self, component: &str) -> bool {
        self.artifact_name(component).is_ok()
            && (self.expected_hash(component).is_some() || self.source_sha256(component).is_some())
    }

    pub fn set_artifact(
        &mut self,
        component: &str,
        field: &str,
        value: impl Into<String>,
    ) -> Result<(), String> {
        let artifacts = self.table_mut("artifacts")?;
        let nested_field = if field == "wasm" { "filename" } else { field };
        if let Some(component_value) = artifacts.get_mut(component) {
            let component_table = component_value
                .as_table_mut()
                .ok_or_else(|| format!("artifacts.{component} must be a table"))?;
            component_table.insert(nested_field.into(), toml::Value::String(value.into()));
            return Ok(());
        }
        let legacy_field = if matches!(field, "filename" | "wasm") {
            format!("{component}_wasm")
        } else {
            format!("{component}_{field}")
        };
        artifacts.insert(legacy_field, toml::Value::String(value.into()));
        Ok(())
    }

    pub fn set_value(
        &mut self,
        section: &str,
        key: &str,
        value: impl Into<String>,
    ) -> Result<(), String> {
        self.table_mut(section)?
            .insert(key.into(), toml::Value::String(value.into()));
        Ok(())
    }

    pub fn set_bool(&mut self, section: &str, key: &str, value: bool) -> Result<(), String> {
        self.table_mut(section)?
            .insert(key.into(), toml::Value::Boolean(value));
        Ok(())
    }

    fn table_mut(&mut self, section: &str) -> Result<&mut toml::Table, String> {
        let root = self
            .document
            .as_table_mut()
            .ok_or_else(|| "manifest root must be a TOML table".to_string())?;
        if !root.contains_key(section) {
            root.insert(section.into(), toml::Value::Table(toml::Table::new()));
        }
        root.get_mut(section)
            .and_then(toml::Value::as_table_mut)
            .ok_or_else(|| format!("manifest section [{section}] must be a table"))
    }
}

impl Default for SnsManifest {
    fn default() -> Self {
        Self {
            document: toml::Value::Table(toml::Table::new()),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FetchMetadata<'a> {
    pub source_url: &'a str,
    pub source_sha256: &'a str,
    pub source_kind: &'a str,
    pub source_filename: Option<&'a str>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoted_hash_is_not_treated_as_a_comment() {
        let manifest = SnsManifest::parse(
            r#"
                [artifacts]
                sns_governance_wasm = "sns_governance.wasm"
                sns_governance_source_url = "local://candidate/path#not-a-comment"
            "#,
        )
        .unwrap();
        assert_eq!(
            manifest.artifact("sns_governance", "source_url"),
            Some("local://candidate/path#not-a-comment")
        );
    }

    #[test]
    fn nested_and_legacy_artifacts_have_one_interpretation() {
        let nested = SnsManifest::parse(
            r#"
                [artifacts.sns_governance]
                filename = "sns_governance.wasm"
                sha256 = "abc"
            "#,
        )
        .unwrap();
        let legacy = SnsManifest::parse(
            r#"
                [artifacts]
                sns_governance_wasm = "sns_governance.wasm"
                sns_governance_sha256 = "abc"
            "#,
        )
        .unwrap();
        for manifest in [&nested, &legacy] {
            assert_eq!(
                manifest.artifact("sns_governance", "filename"),
                Some("sns_governance.wasm")
            );
            assert_eq!(manifest.artifact("sns_governance", "sha256"), Some("abc"));
        }
    }

    #[test]
    fn capability_accepts_toml_bool_and_legacy_string() {
        let current = SnsManifest::parse("[capabilities]\nfeature = true\n").unwrap();
        let legacy = SnsManifest::parse("[capabilities]\nfeature = \"false\"\n").unwrap();
        assert_eq!(current.bool_value("capabilities", "feature"), Some(true));
        assert_eq!(legacy.bool_value("capabilities", "feature"), Some(false));
    }
}
