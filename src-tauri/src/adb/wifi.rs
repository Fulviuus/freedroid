//! Wireless (Wi-Fi) ADB connectivity.
//!
//! Two supported flows:
//!  * Legacy: device plugged in once, switch its adb daemon to TCP/IP
//!    (`adb tcpip 5555`), read its Wi-Fi IP, then `adb connect ip:5555`.
//!  * Android 11+ wireless debugging: pair with a code (`adb pair ip:port code`)
//!    then `adb connect ip:port`. Note the pairing port differs from the
//!    connect port — both are shown on the device's Wireless Debugging screen.

use crate::adb::{run, run_on};
use crate::error::{Error, Result};
use tauri::AppHandle;

/// Switch a USB-connected device to TCP/IP mode and return its `ip:5555`
/// address, ready to pass to `connect`.
pub async fn enable_tcpip(app: &AppHandle, serial: &str) -> Result<String> {
    let ip = device_wifi_ip(app, serial).await?;
    run_on(app, serial, &["tcpip", "5555"]).await?;
    Ok(format!("{ip}:5555"))
}

/// Read the device's wlan0 IPv4 address.
pub async fn device_wifi_ip(app: &AppHandle, serial: &str) -> Result<String> {
    let out = run_on(
        app,
        serial,
        &["shell", "ip -f inet addr show wlan0 2>/dev/null"],
    )
    .await?;
    // Look for: "    inet 192.168.1.5/24 brd ..."
    for line in out.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("inet ") {
            if let Some(cidr) = rest.split_whitespace().next() {
                if let Some(ip) = cidr.split('/').next() {
                    return Ok(ip.to_string());
                }
            }
        }
    }
    Err(Error::Adb(
        "could not determine device Wi-Fi IP (is Wi-Fi on?)".into(),
    ))
}

pub async fn connect(app: &AppHandle, address: &str) -> Result<String> {
    let out = run(app, &["connect", address]).await?;
    if out.to_lowercase().contains("cannot")
        || out.to_lowercase().contains("failed")
        || out.to_lowercase().contains("unable")
    {
        return Err(Error::Adb(out.trim().to_string()));
    }
    Ok(out.trim().to_string())
}

pub async fn disconnect(app: &AppHandle, address: &str) -> Result<()> {
    run(app, &["disconnect", address]).await?;
    Ok(())
}

/// Android 11+ wireless pairing. `address` is `ip:pairingPort`, `code` is the
/// 6-digit pairing code shown on the device.
pub async fn pair(app: &AppHandle, address: &str, code: &str) -> Result<String> {
    let out = run(app, &["pair", address, code]).await?;
    if out.to_lowercase().contains("fail") {
        return Err(Error::Adb(out.trim().to_string()));
    }
    Ok(out.trim().to_string())
}
