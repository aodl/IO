fn main() {
    io_build_support::emit_build_metadata();
    println!("cargo:rerun-if-env-changed=IO_FRONTEND_PUBLIC_DIR");
    let public = std::env::var_os("IO_FRONTEND_PUBLIC_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("public")
        });
    let public = public.canonicalize().expect("frontend public directory");
    println!("cargo:rerun-if-changed={}", public.display());
    println!(
        "cargo:rustc-env=IO_FRONTEND_PUBLIC_DIR={}",
        public.display()
    );
}
