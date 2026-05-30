//! Manual MTP smoke test. Run with:
//!   cargo run --features mtp --example mtp_probe
//! Requires the device in "File Transfer (MTP)" USB mode.

#[cfg(feature = "mtp")]
fn main() {
    match freedroid_lib::mtp::probe() {
        Ok(report) => println!("{report}"),
        Err(e) => {
            eprintln!("MTP probe failed: {e}");
            std::process::exit(1);
        }
    }
}

#[cfg(not(feature = "mtp"))]
fn main() {
    eprintln!("Rebuild with: cargo run --features mtp --example mtp_probe");
    std::process::exit(1);
}
