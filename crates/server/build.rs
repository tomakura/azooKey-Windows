use std::env;
use std::path::PathBuf;

fn main() {
    // Link the Swift converter (azookey-server.dll import library) built by
    // `swift build -c release` in server-swift (cargo-make task: build_swift).
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let swift_release = manifest_dir.join("../../server-swift/.build/x86_64-unknown-windows-msvc/release");
    println!("cargo:rustc-link-search={}", swift_release.display());
    println!("cargo:rustc-link-lib=azookey-server");
}
