use candid::{decode_one, encode_one};
use ic_http_certification::{HttpRequest, HttpResponse, StatusCode};
use pocket_ic::PocketIc;

const CYCLES: u128 = 2_000_000_000_000;

fn pocketic_available() -> bool {
    std::env::var_os("POCKET_IC_BIN").is_some()
}

fn required_wasm(path: &str) -> Option<Vec<u8>> {
    match std::fs::read(path) {
        Ok(bytes) => Some(bytes),
        Err(_) => {
            eprintln!("skipping frontend PocketIC test because {path} is missing");
            None
        }
    }
}

fn query_http(pic: &PocketIc, canister: candid::Principal, path: &str) -> HttpResponse<'static> {
    let bytes = pic
        .query_call(
            canister,
            candid::Principal::anonymous(),
            "http_request",
            encode_one(HttpRequest::get(path).build()).unwrap(),
        )
        .expect("query frontend http_request");
    decode_one(&bytes).unwrap()
}

#[test]
fn pocketic_frontend_serves_certified_assets_and_404() {
    if !pocketic_available() {
        eprintln!("skipping frontend PocketIC test because POCKET_IC_BIN is not set");
        return;
    }

    let wasm = match required_wasm("target/wasm32-unknown-unknown/debug/io_frontend.wasm") {
        Some(wasm) => wasm,
        None => return,
    };

    for path in [
        "canisters/io_stream_manager/io_stream_manager.did",
        "canisters/io_nns_neuron_manager/io_nns_neuron_manager.did",
    ] {
        let did = std::fs::read_to_string(path).expect("value-moving DID should be readable");
        assert!(did.contains("service : (InitArgs) -> {}"));
        assert!(!did.contains("debug_"));
        assert!(!did.contains(" get_state :"));
    }

    let pic = PocketIc::new();
    let frontend = pic.create_canister();
    pic.add_cycles(frontend, CYCLES);
    pic.install_canister(frontend, wasm, encode_one(()).unwrap(), None);

    let index = query_http(&pic, frontend, "/");
    assert_eq!(index.status_code(), StatusCode::OK);
    let index_html = String::from_utf8_lossy(index.body());
    assert!(index_html.contains("REAL LIQUID STAKING"));
    let start = index_html.find("/generated/app.").expect("bundle path");
    let end = index_html[start..].find(".js").expect("bundle suffix") + start + 3;
    let bundle_path = &index_html[start..end];

    let bundle = query_http(&pic, frontend, bundle_path);
    assert_eq!(bundle.status_code(), StatusCode::OK);
    assert!(bundle
        .headers()
        .iter()
        .any(|(name, value)| name.eq_ignore_ascii_case("Cache-Control")
            && value.contains("immutable")));

    let missing = query_http(&pic, frontend, "/missing");
    assert_eq!(missing.status_code(), StatusCode::NOT_FOUND);

    let manifest = query_http(&pic, frontend, "/generated/frontend-bundle.json");
    assert_eq!(manifest.status_code(), StatusCode::NOT_FOUND);
}
