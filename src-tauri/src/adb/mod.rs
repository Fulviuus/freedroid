//! Thin async wrapper around the bundled `adb` sidecar binary.
//!
//! All device communication goes through here. Commands are run via the Tauri
//! shell plugin so the sidecar path resolves correctly in both dev and bundled
//! builds, and so the call is checked against the `shell:allow-execute`
//! capability.

pub mod devices;
pub mod files;
pub mod transfer;
pub mod wifi;

use crate::error::{Error, Result};
use tauri::AppHandle;
use tauri_plugin_shell::ShellExt;

/// Run an adb command and return stdout as a string. Errors if adb exits
/// non-zero, surfacing stderr to the caller.
pub async fn run(app: &AppHandle, args: &[&str]) -> Result<String> {
    let cmd = app.shell().sidecar("adb")?;
    let output = cmd.args(args).output().await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let msg = if stderr.is_empty() { stdout } else { stderr };
        return Err(Error::Adb(if msg.is_empty() {
            "command failed".into()
        } else {
            msg
        }));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Run an adb command scoped to a specific device serial (`adb -s <serial> ...`).
pub async fn run_on(app: &AppHandle, serial: &str, args: &[&str]) -> Result<String> {
    let mut full: Vec<&str> = vec!["-s", serial];
    full.extend_from_slice(args);
    run(app, &full).await
}

/// Best-effort: stop the adb server. Call on app exit to avoid orphaned
/// background servers (adb listens on port 5037).
pub async fn kill_server(app: &AppHandle) {
    let _ = run(app, &["kill-server"]).await;
}

/// Quote a string for safe inclusion in a single `adb shell "<cmd>"` argument.
/// adb concatenates shell args with spaces and re-parses them on the device,
/// so we wrap paths in single quotes and escape embedded single quotes.
pub fn shell_quote(s: &str) -> String {
    let escaped = s.replace('\'', r"'\''");
    format!("'{escaped}'")
}

/// Reject paths that escape the user-visible storage area or attempt traversal.
/// Device file operations are confined to /sdcard and /storage.
pub fn validate_device_path(path: &str) -> Result<()> {
    if !(path == "/sdcard"
        || path == "/storage"
        || path.starts_with("/sdcard/")
        || path.starts_with("/storage/"))
    {
        return Err(Error::InvalidPath(format!(
            "{path} is outside allowed device storage"
        )));
    }
    if path.split('/').any(|seg| seg == "..") {
        return Err(Error::InvalidPath("path traversal not allowed".into()));
    }
    Ok(())
}
