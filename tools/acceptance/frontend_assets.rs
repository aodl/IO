//! Shared offline checks used by checkpoint A and the installed-asset tests.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

pub fn acceptance_config() -> Value {
    json!({"network":"ic", "historianCanisterId":"gaqmf-jaaaa-aaaas-amxva-cai",
        "streamManagerCanisterId":"gjthz-7iaaa-aaaas-amxuq-cai",
        "ioLedgerCanisterId":"hn6il-giaaa-aaaas-amxsq-cai",
        "frontendCanisterId":"ghrkr-eyaaa-aaaas-amxvq-cai", "demoMode":false})
}

pub fn validate(public: &Path, wasm: &[u8], expected: &Value) -> Result<Value, String> {
    let read = |path: &Path| fs::read(path).map_err(|e| format!("{}: {e}", path.display()));
    let manifest: Value =
        serde_json::from_slice(&read(&public.join("generated/frontend-bundle.json"))?)
            .map_err(|e| e.to_string())?;
    let mut config = manifest["config"].clone();
    config
        .as_object_mut()
        .ok_or("browser config missing")?
        .remove("assetVersion");
    if config != *expected {
        return Err("browser configuration differs from acceptance".into());
    }
    let hash = manifest["sha256"].as_str().ok_or("bundle hash missing")?;
    let path = format!("/generated/app.{hash}.js");
    if hash.len() != 12
        || !hash.bytes().all(|c| c.is_ascii_hexdigit())
        || manifest["bundle"] != path
        || manifest["config"]["assetVersion"] != hash
    {
        return Err("invalid content-addressed bundle path".into());
    }
    let html = read(&public.join("index.html"))?;
    if !String::from_utf8_lossy(&html).contains(&format!("src=\"{path}\"")) {
        return Err("index does not reference the generated bundle".into());
    }
    let bundle = read(&public.join(&path[1..]))?;
    let digest = hex::encode(Sha256::digest(&bundle));
    if &digest[..12] != hash {
        return Err("browser bundle content hash mismatch".into());
    }
    config["assetVersion"] = json!("");
    // esbuild preserves JSON.parse's JSON literal. Check the executable input,
    // not just the independently editable manifest.
    if !String::from_utf8_lossy(&bundle)
        .split("JSON.parse('")
        .any(|tail| {
            tail.split_once("')")
                .and_then(|(s, _)| serde_json::from_str::<Value>(s).ok())
                .is_some_and(|actual| actual == config)
        })
    {
        return Err("executable browser configuration mismatch".into());
    }
    for asset in [&html, &bundle] {
        if asset.is_empty() || !wasm.windows(asset.len()).any(|window| window == asset) {
            return Err("frontend Wasm does not embed the checked browser assets".into());
        }
    }
    Ok(
        json!({"config":expected,"bundle":path,"bundle_sha256":digest,
        "index_sha256":hex::encode(Sha256::digest(&html)),
        "wasm_sha256":hex::encode(Sha256::digest(wasm))}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acceptance_browser_binding_rejects_identity_and_asset_contamination() {
        // Component fixture only. The PocketIC test checks real generated assets.
        let public = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target")
            .join(format!(
                "frontend-binding-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
        fs::create_dir_all(public.join("generated")).unwrap();
        let expected = acceptance_config();
        let mut config = expected.clone();
        config["assetVersion"] = json!("");
        let bundle = format!("let config=JSON.parse('{}');", config);
        let hash = &hex::encode(Sha256::digest(bundle.as_bytes()))[..12];
        let path = format!("/generated/app.{hash}.js");
        let html = format!("<script src=\"{path}\"></script>");
        let wasm = [
            b"component wasm".as_slice(),
            html.as_bytes(),
            bundle.as_bytes(),
        ]
        .concat();
        config["assetVersion"] = json!(hash);
        let manifest = json!({"bundle":path,"sha256":hash,"config":config});
        let manifest_path = public.join("generated/frontend-bundle.json");
        fs::write(&manifest_path, manifest.to_string()).unwrap();
        fs::write(public.join("index.html"), &html).unwrap();
        fs::write(public.join(&path[1..]), &bundle).unwrap();
        validate(&public, &wasm, &expected).unwrap();
        for (key, value) in [
            ("network", json!("local")),
            ("demoMode", json!(true)),
            ("historianCanisterId", json!("wrong")),
            ("streamManagerCanisterId", json!("wrong")),
            ("ioLedgerCanisterId", json!("wrong")),
            ("frontendCanisterId", json!("wrong")),
        ] {
            let mut bad = manifest.clone();
            bad["config"][key] = value;
            fs::write(&manifest_path, bad.to_string()).unwrap();
            assert!(
                validate(&public, &wasm, &expected).is_err(),
                "accepted {key}"
            );
        }
        fs::write(&manifest_path, manifest.to_string()).unwrap();
        assert!(validate(&public, b"different wasm", &expected).is_err());
        fs::write(
            public.join("index.html"),
            "<script src=\"/missing.js\"></script>",
        )
        .unwrap();
        assert!(validate(&public, &wasm, &expected).is_err());
        fs::write(public.join("index.html"), &html).unwrap();
        fs::write(public.join(&path[1..]), "different bytes").unwrap();
        assert!(validate(&public, &wasm, &expected).is_err());
        fs::remove_file(public.join(&path[1..])).unwrap();
        assert!(validate(&public, &wasm, &expected).is_err());
        // Even a coherent HTML/hash/Wasm set cannot hide a different executable
        // configuration behind an acceptance-labelled manifest.
        let mut wrong_config = expected.clone();
        wrong_config["network"] = json!("local");
        wrong_config["assetVersion"] = json!("");
        let wrong_bundle = format!("let config=JSON.parse('{}');", wrong_config);
        let wrong_hash = &hex::encode(Sha256::digest(wrong_bundle.as_bytes()))[..12];
        let wrong_path = format!("/generated/app.{wrong_hash}.js");
        let wrong_html = format!("<script src=\"{wrong_path}\"></script>");
        let mut labelled = manifest;
        labelled["bundle"] = json!(wrong_path);
        labelled["sha256"] = json!(wrong_hash);
        labelled["config"]["assetVersion"] = json!(wrong_hash);
        fs::write(&manifest_path, labelled.to_string()).unwrap();
        fs::write(public.join("index.html"), &wrong_html).unwrap();
        fs::write(public.join(&wrong_path[1..]), &wrong_bundle).unwrap();
        let wrong_wasm = [wrong_html.as_bytes(), wrong_bundle.as_bytes()].concat();
        assert_eq!(
            validate(&public, &wrong_wasm, &expected).unwrap_err(),
            "executable browser configuration mismatch"
        );
        fs::remove_dir_all(public).unwrap();
    }
}
