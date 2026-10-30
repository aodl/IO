//! Shared acceptance-only memo selection for constructors, Accounts and funding fixtures.
//! The deployment JSON is the single configuration input; production is untouched.
pub fn memo(root: &std::path::Path) -> Result<u64, String> {
    let path = root.join("deploy/acceptance/io-init-configs.draft.json");
    let init: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
    init["nns_manager"]["pooled_parent_memo"]
        .as_u64()
        .filter(|memo| *memo == 1)
        .ok_or_else(|| "acceptance Dynamic parent requires reviewed memo 1".into())
}
