use std::env;

fn main() {
    let version = env::var("VERSION").unwrap_or(env!("CARGO_PKG_VERSION").to_string());
    println!("cargo:rustc-env=CARGO_PKG_VERSION={}", version);
}
