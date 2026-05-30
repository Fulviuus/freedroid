fn main() {
    // Link libmtp (+ libusb, IOKit) only when the optional MTP backend is built.
    if std::env::var_os("CARGO_FEATURE_MTP").is_some() {
        if let Err(e) = pkg_config::Config::new().probe("libmtp") {
            println!("cargo:warning=libmtp not found via pkg-config: {e}");
        }
        // Search the bundled Frameworks dir at runtime, so the dylibs we copy
        // into the .app (see scripts/bundle-mtp-dylibs.sh) are found first.
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../Frameworks");
    }
    tauri_build::build()
}
