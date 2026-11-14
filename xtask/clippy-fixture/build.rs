//! Declares the `lumen_clippy_fixture` cfg, which only `cargo run -p xtask -- clippy-bans` sets.

fn main() {
    println!("cargo::rustc-check-cfg=cfg(lumen_clippy_fixture)");
}
