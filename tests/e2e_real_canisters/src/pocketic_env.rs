use candid::Principal;
use pocket_ic::{PocketIc, PocketIcBuilder};

const CYCLES: u128 = 2_000_000_000_000;

pub fn pocketic_available() -> bool {
    std::env::var_os("POCKET_IC_BIN").is_some()
}

pub fn new_sns_pic() -> PocketIc {
    PocketIcBuilder::new().with_sns_subnet().build()
}

pub fn create_canister(pic: &PocketIc, wasm: Vec<u8>, arg: Vec<u8>) -> Principal {
    let canister = pic.create_canister();
    pic.add_cycles(canister, CYCLES);
    pic.install_canister(canister, wasm, arg, None);
    canister
}

pub fn upgrade_canister(pic: &PocketIc, canister: Principal, wasm: Vec<u8>, arg: Vec<u8>) {
    pic.upgrade_canister(canister, wasm, arg, None)
        .expect("same-Wasm upgrade should succeed");
}
