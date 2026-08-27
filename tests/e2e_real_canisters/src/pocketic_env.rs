use candid::Principal;
use pocket_ic::{
    common::rest::{IcpFeatures, IcpFeaturesConfig},
    PocketIc, PocketIcBuilder,
};

const CYCLES: u128 = 2_000_000_000_000;

pub fn pocketic_available() -> bool {
    std::env::var_os("POCKET_IC_BIN").is_some()
}

pub fn new_sns_pic() -> PocketIc {
    with_optional_server_url(PocketIcBuilder::new())
        .with_nns_subnet()
        .with_sns_subnet()
        .with_application_subnet()
        .build()
}

pub fn new_pic_with_icp_sns_features() -> PocketIc {
    with_optional_server_url(PocketIcBuilder::new())
        .with_application_subnet()
        .with_icp_features(IcpFeatures {
            registry: Some(IcpFeaturesConfig::DefaultConfig),
            cycles_minting: Some(IcpFeaturesConfig::DefaultConfig),
            icp_token: Some(IcpFeaturesConfig::DefaultConfig),
            nns_governance: Some(IcpFeaturesConfig::DefaultConfig),
            sns: Some(IcpFeaturesConfig::DefaultConfig),
            ..Default::default()
        })
        .build()
}

pub fn new_pic_with_nns_governance_features() -> PocketIc {
    with_optional_server_url(PocketIcBuilder::new())
        .with_sns_subnet()
        .with_application_subnet()
        .with_icp_features(IcpFeatures {
            registry: Some(IcpFeaturesConfig::DefaultConfig),
            cycles_minting: Some(IcpFeaturesConfig::DefaultConfig),
            icp_token: Some(IcpFeaturesConfig::DefaultConfig),
            nns_governance: Some(IcpFeaturesConfig::DefaultConfig),
            ..Default::default()
        })
        .build()
}

pub fn new_pic_with_nns_governance_and_fiduciary_features() -> PocketIc {
    with_optional_server_url(PocketIcBuilder::new())
        .with_sns_subnet()
        .with_fiduciary_subnet()
        .with_application_subnet()
        .with_icp_features(IcpFeatures {
            registry: Some(IcpFeaturesConfig::DefaultConfig),
            cycles_minting: Some(IcpFeaturesConfig::DefaultConfig),
            icp_token: Some(IcpFeaturesConfig::DefaultConfig),
            nns_governance: Some(IcpFeaturesConfig::DefaultConfig),
            ..Default::default()
        })
        .build()
}

fn with_optional_server_url(builder: PocketIcBuilder) -> PocketIcBuilder {
    let builder = builder.with_max_request_time_ms(Some(900_000));
    match std::env::var("POCKET_IC_SERVER_URL") {
        Ok(server_url) => builder.with_server_url(
            server_url
                .parse()
                .expect("POCKET_IC_SERVER_URL should be a valid URL"),
        ),
        Err(_) => builder,
    }
}

pub fn create_sns_canister(pic: &PocketIc, wasm: Vec<u8>, arg: Vec<u8>) -> Principal {
    let sns_subnet = pic.topology().get_sns().expect("SNS subnet should exist");
    create_canister_on_subnet(pic, sns_subnet, wasm, arg)
}

pub fn create_application_canister(pic: &PocketIc, wasm: Vec<u8>, arg: Vec<u8>) -> Principal {
    let app_subnet = pic
        .topology()
        .get_app_subnets()
        .into_iter()
        .next()
        .expect("application subnet should exist");
    create_canister_on_subnet(pic, app_subnet, wasm, arg)
}

pub fn create_empty_application_canister(pic: &PocketIc) -> Principal {
    let app_subnet = pic
        .topology()
        .get_app_subnets()
        .into_iter()
        .next()
        .expect("application subnet should exist");
    let canister = pic.create_canister_on_subnet(None, None, app_subnet);
    pic.add_cycles(canister, CYCLES);
    canister
}

fn create_canister_on_subnet(
    pic: &PocketIc,
    subnet: Principal,
    wasm: Vec<u8>,
    arg: Vec<u8>,
) -> Principal {
    let canister = pic.create_canister_on_subnet(None, None, subnet);
    pic.add_cycles(canister, CYCLES);
    pic.install_canister(canister, wasm, arg, None);
    canister
}

pub fn upgrade_canister(pic: &PocketIc, canister: Principal, wasm: Vec<u8>, arg: Vec<u8>) {
    pic.upgrade_canister(canister, wasm, arg, None)
        .expect("same-Wasm upgrade should succeed");
}
