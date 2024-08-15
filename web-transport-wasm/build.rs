fn main() {
    // Required for WebTransport features on docs.rs
    println!("cargo:rustc-cfg=web_sys_unstable_apis");
}
