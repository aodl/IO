pub fn emit_build_metadata() {
    println!("cargo:rerun-if-changed=build.rs");
}
