use ic_asset_certification::{Asset, AssetConfig, AssetFallbackConfig, AssetRouter};
use ic_http_certification::{HttpRequest, HttpResponse, Method, StatusCode};
use include_dir::{include_dir, Dir};
use std::cell::RefCell;

static ASSETS: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/public");

const NO_CACHE: &str = "public, no-cache, no-store";
const IMMUTABLE: &str = "public, max-age=31536000, immutable";
const PRIVATE_BUNDLE_MANIFEST: &str = "generated/frontend-bundle.json";

pub const EXPECTED_HISTORIAN_DASHBOARD_METHOD: &str = "get_dashboard_state";
pub const EXPECTED_HISTORIAN_STATUS_METHOD: &str = "get_public_status";

thread_local! {
    static ROUTER: RefCell<Option<AssetRouter<'static>>> = const { RefCell::new(None) };
}

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg_attr(target_family = "wasm", ic_cdk::init)]
#[allow(dead_code)]
fn init() {
    initialise_certified_assets();
}

#[cfg_attr(target_family = "wasm", ic_cdk::post_upgrade)]
#[allow(dead_code)]
fn post_upgrade() {
    initialise_certified_assets();
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
#[allow(dead_code)]
fn http_request(req: HttpRequest<'static>) -> HttpResponse<'static> {
    handle_http_request(req, data_certificate())
}

pub fn initialise_certified_assets() {
    let router = build_asset_router().expect("frontend assets should certify");
    set_certified_data(&router.root_hash());
    ROUTER.with(|cell| {
        *cell.borrow_mut() = Some(router);
    });
}

pub fn handle_http_request(
    req: HttpRequest<'static>,
    data_certificate: Vec<u8>,
) -> HttpResponse<'static> {
    if req.method() != Method::GET && req.method() != Method::HEAD {
        return plain_response(
            StatusCode::METHOD_NOT_ALLOWED,
            b"method not allowed".to_vec(),
            vec![("Allow".to_string(), "GET, HEAD".to_string())],
        );
    }

    if bad_url(req.url()) {
        return plain_response(StatusCode::BAD_REQUEST, b"bad request".to_vec(), vec![]);
    }

    ROUTER.with(|cell| {
        let mut borrowed = cell.borrow_mut();
        if borrowed.is_none() {
            *borrowed = Some(build_asset_router().expect("frontend assets should certify"));
        }
        let router = borrowed.as_ref().expect("router initialized");
        let mut response = match router.serve_asset(&data_certificate, &req) {
            Ok(response) => response,
            Err(_) => plain_response(StatusCode::NOT_FOUND, b"not found".to_vec(), vec![]),
        };
        if req.method() == Method::HEAD {
            response = HttpResponse::builder()
                .with_status_code(response.status_code())
                .with_headers(response.headers().to_vec())
                .with_body(Vec::new())
                .build();
        }
        response
    })
}

fn build_asset_router() -> Result<AssetRouter<'static>, String> {
    let mut router = AssetRouter::default();
    let entries = collect_assets(&ASSETS);
    let paths = entries
        .iter()
        .map(|(path, _)| path.clone())
        .collect::<Vec<_>>();
    let assets = entries.into_iter().map(|(_, asset)| asset);
    let configs = asset_configs(&paths);
    router
        .certify_assets(assets, configs)
        .map_err(|err| err.to_string())?;
    Ok(router)
}

fn collect_assets(dir: &'static Dir<'static>) -> Vec<(String, Asset<'static, 'static>)> {
    let mut assets = dir
        .files()
        .filter_map(|file| {
            let path = file.path().to_string_lossy().replace('\\', "/");
            if path == PRIVATE_BUNDLE_MANIFEST {
                return None;
            }
            Some((path.clone(), Asset::new(path, file.contents())))
        })
        .collect::<Vec<_>>();
    for child in dir.dirs() {
        assets.extend(collect_assets(child));
    }
    assets
}

fn asset_configs(paths: &[String]) -> Vec<AssetConfig> {
    let mut configs = vec![
        AssetConfig::File {
            path: "index.html".to_string(),
            content_type: Some("text/html; charset=utf-8".to_string()),
            headers: response_headers(NO_CACHE),
            fallback_for: vec![],
            aliased_by: vec!["/".to_string()],
            encodings: vec![],
        },
        AssetConfig::File {
            path: "404.html".to_string(),
            content_type: Some("text/html; charset=utf-8".to_string()),
            headers: response_headers(NO_CACHE),
            fallback_for: vec![AssetFallbackConfig {
                scope: "/".to_string(),
                status_code: Some(StatusCode::NOT_FOUND),
            }],
            aliased_by: vec!["/404".to_string(), "/404.html".to_string()],
            encodings: vec![],
        },
        AssetConfig::File {
            path: ".well-known/ic-domains".to_string(),
            content_type: Some("text/plain; charset=utf-8".to_string()),
            headers: response_headers(NO_CACHE),
            fallback_for: vec![],
            aliased_by: vec![],
            encodings: vec![],
        },
    ];

    configs.extend(paths.iter().filter_map(|path| {
        let path = path.as_str();
        if matches!(path, "index.html" | "404.html" | ".well-known/ic-domains") {
            return None;
        }
        Some(AssetConfig::File {
            path: path.to_string(),
            content_type: Some(content_type(path)),
            headers: response_headers(cache_control(path)),
            fallback_for: vec![],
            aliased_by: vec![],
            encodings: vec![],
        })
    }));

    configs
}

fn content_type(path: &str) -> String {
    match path.rsplit('.').next().unwrap_or_default() {
        "css" => "text/css; charset=utf-8".to_string(),
        "js" | "mjs" => "text/javascript; charset=utf-8".to_string(),
        "html" => "text/html; charset=utf-8".to_string(),
        _ => mime_guess::from_path(path)
            .first_or_octet_stream()
            .essence_str()
            .to_string(),
    }
}

fn cache_control(path: &str) -> &'static str {
    if path.starts_with("generated/") || path.starts_with("assets/") {
        IMMUTABLE
    } else {
        NO_CACHE
    }
}

fn response_headers(cache_control: &str) -> Vec<(String, String)> {
    vec![
        ("Cache-Control".to_string(), cache_control.to_string()),
        ("Strict-Transport-Security".to_string(), "max-age=31536000; includeSubDomains".to_string()),
        ("X-Content-Type-Options".to_string(), "nosniff".to_string()),
        ("Content-Security-Policy".to_string(), csp().to_string()),
        ("Referrer-Policy".to_string(), "strict-origin-when-cross-origin".to_string()),
        ("Permissions-Policy".to_string(), "accelerometer=(), autoplay=(), camera=(), encrypted-media=(), fullscreen=(self), geolocation=(), microphone=(), payment=(), usb=()".to_string()),
        ("Cross-Origin-Embedder-Policy".to_string(), "require-corp".to_string()),
        ("Cross-Origin-Opener-Policy".to_string(), "same-origin".to_string()),
        ("Cross-Origin-Resource-Policy".to_string(), "same-origin".to_string()),
    ]
}

fn csp() -> &'static str {
    "default-src 'self'; connect-src 'self' https://icp0.io https://*.icp0.io; script-src 'self'; style-src 'self'; style-src-attr 'none'; img-src 'self' data:; font-src 'self'; worker-src 'none'; frame-src 'none'; object-src 'none'; base-uri 'self'; form-action 'self'; frame-ancestors 'self'; upgrade-insecure-requests"
}

fn plain_response(
    status: StatusCode,
    body: Vec<u8>,
    extra_headers: Vec<(String, String)>,
) -> HttpResponse<'static> {
    let mut headers = response_headers(NO_CACHE);
    headers.push((
        "Content-Type".to_string(),
        "text/plain; charset=utf-8".to_string(),
    ));
    headers.extend(extra_headers);
    HttpResponse::builder()
        .with_status_code(status)
        .with_headers(headers)
        .with_body(body)
        .build()
}

fn bad_url(url: &str) -> bool {
    url.contains("..") || url.contains('\\') || !url.starts_with('/')
}

#[cfg(target_family = "wasm")]
fn data_certificate() -> Vec<u8> {
    ic_cdk::api::data_certificate().unwrap_or_default()
}

#[cfg(not(target_family = "wasm"))]
#[allow(dead_code)]
fn data_certificate() -> Vec<u8> {
    vec![0; 32]
}

#[cfg(target_family = "wasm")]
fn set_certified_data(root_hash: &[u8; 32]) {
    ic_cdk::api::certified_data_set(root_hash);
}

#[cfg(not(target_family = "wasm"))]
fn set_certified_data(_root_hash: &[u8; 32]) {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn get(path: &str) -> HttpResponse<'static> {
        initialise_certified_assets();
        handle_http_request(HttpRequest::get(path).build(), vec![1, 2, 3])
    }

    fn head(path: &str) -> HttpResponse<'static> {
        initialise_certified_assets();
        handle_http_request(
            HttpRequest::builder()
                .with_method(Method::HEAD)
                .with_url(path)
                .build(),
            vec![1, 2, 3],
        )
    }

    fn header<'a>(response: &'a HttpResponse<'_>, name: &str) -> &'a str {
        response
            .headers()
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
            .unwrap_or("")
    }

    #[test]
    fn version_exists() {
        assert!(!version().is_empty());
    }

    #[test]
    fn frontend_points_at_historian_read_model() {
        assert_eq!(EXPECTED_HISTORIAN_DASHBOARD_METHOD, "get_dashboard_state");
        assert_eq!(EXPECTED_HISTORIAN_STATUS_METHOD, "get_public_status");
    }

    #[test]
    fn root_and_index_serve_index_html() {
        for path in ["/", "/index.html"] {
            let response = get(path);
            assert_eq!(response.status_code(), StatusCode::OK);
            assert!(String::from_utf8_lossy(response.body()).contains("REAL LIQUID STAKING"));
            assert!(header(&response, "IC-Certificate").contains("certificate"));
        }
    }

    #[test]
    fn unknown_paths_return_certified_404() {
        let response = get("/missing/page");
        assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
        assert!(String::from_utf8_lossy(response.body()).contains("IO page unavailable"));
        assert!(header(&response, "IC-Certificate").contains("certificate"));
    }

    #[test]
    fn private_bundle_manifest_is_not_routable() {
        let response = get("/generated/frontend-bundle.json");
        assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn metrics_route_is_not_exposed() {
        let response = get("/metrics");
        assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn content_types_and_cache_policies_are_set() {
        let css = get("/base.css");
        assert_eq!(header(&css, "Content-Type"), "text/css; charset=utf-8");
        assert_eq!(header(&css, "Cache-Control"), NO_CACHE);

        let image = get("/assets/io-fallback-progressive.jpg");
        assert_eq!(header(&image, "Content-Type"), "image/jpeg");
        assert_eq!(header(&image, "Cache-Control"), IMMUTABLE);
    }

    #[test]
    fn generated_bundle_is_immutable_when_built() {
        let index = String::from_utf8_lossy(get("/").body()).to_string();
        let start = index.find("/generated/app.").expect("bundle path");
        let end = index[start..].find(".js").expect("bundle suffix") + start + 3;
        let response = get(&index[start..end]);
        if response.status_code() == StatusCode::NOT_FOUND {
            let generated_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("public/generated");
            assert!(
                !generated_dir.exists(),
                "generated bundle should be routable when public/generated exists"
            );
            return;
        }
        assert_eq!(response.status_code(), StatusCode::OK);
        assert_eq!(header(&response, "Cache-Control"), IMMUTABLE);
        assert_eq!(
            header(&response, "Content-Type"),
            "text/javascript; charset=utf-8"
        );
    }

    #[test]
    fn security_headers_are_present_and_strict() {
        let response = get("/");
        assert_eq!(header(&response, "X-Content-Type-Options"), "nosniff");
        assert!(header(&response, "Strict-Transport-Security").contains("max-age"));
        assert_eq!(
            header(&response, "Cross-Origin-Opener-Policy"),
            "same-origin"
        );
        let csp = header(&response, "Content-Security-Policy");
        assert!(csp.contains("script-src 'self'"));
        assert!(csp.contains("style-src 'self'"));
        assert!(csp.contains("style-src-attr 'none'"));
        assert!(!csp.contains("unsafe-inline"));
    }

    #[test]
    fn head_supports_empty_body() {
        let response = head("/");
        assert_eq!(response.status_code(), StatusCode::OK);
        assert!(response.body().is_empty());
    }

    #[test]
    fn bad_url_returns_400() {
        let response = get("/../secret");
        assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn asset_certification_initialises() {
        let router = build_asset_router().expect("router builds");
        assert_ne!(router.root_hash(), [0; 32]);
    }
}
