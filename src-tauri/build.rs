fn main() {
    println!("cargo:rerun-if-changed=resources/fella-python-sandbox.wasm");
    println!("cargo:rerun-if-changed=../python-sandbox/Cargo.toml");
    println!("cargo:rerun-if-changed=../python-sandbox/src/lib.rs");
    tauri_build::build()
}
