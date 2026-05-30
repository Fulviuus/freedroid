fn main() {
    // Link libmtp (+ libusb, IOKit) only when the optional MTP backend is built.
    if std::env::var_os("CARGO_FEATURE_MTP").is_some() {
        if let Err(e) = pkg_config::Config::new().probe("libmtp") {
            println!("cargo:warning=libmtp not found via pkg-config: {e}");
        }
    }
    tauri_build::build()
}
