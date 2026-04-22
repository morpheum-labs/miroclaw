// Web dashboard and bundled `web/dist` were removed. Keep a minimal build script
// for future compile-time steps (e.g. schema generation).
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
}
