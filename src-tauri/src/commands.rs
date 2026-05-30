//! Tauri command surface — the bridge invoked from the Svelte frontend.

use crate::adb::{self, devices::Device, files::DirEntry, files::Volume, wifi};
use crate::error::{Error, Result};
use crate::local::{self, LocalEntry};
use crate::mtp::{DeviceInfo, Entry as MtpEntry, Mtp};
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn adb_version(app: AppHandle) -> Result<String> {
    let out = adb::run(&app, &["version"]).await?;
    Ok(out.lines().next().unwrap_or("").trim().to_string())
}

#[tauri::command]
pub async fn list_devices(app: AppHandle) -> Result<Vec<Device>> {
    adb::devices::list_devices(&app).await
}

#[tauri::command]
pub async fn list_device_dir(app: AppHandle, serial: String, path: String) -> Result<Vec<DirEntry>> {
    adb::files::list_dir(&app, &serial, &path).await
}

#[tauri::command]
pub async fn list_volumes(app: AppHandle, serial: String) -> Result<Vec<Volume>> {
    adb::files::list_volumes(&app, &serial).await
}

#[tauri::command]
pub async fn device_make_dir(app: AppHandle, serial: String, path: String) -> Result<()> {
    adb::files::make_dir(&app, &serial, &path).await
}

#[tauri::command]
pub async fn device_remove(app: AppHandle, serial: String, path: String) -> Result<()> {
    adb::files::remove(&app, &serial, &path).await
}

#[tauri::command]
pub async fn device_remove_many(
    app: AppHandle,
    serial: String,
    paths: Vec<String>,
) -> Result<()> {
    adb::files::remove_many(&app, &serial, &paths).await
}

#[tauri::command]
pub async fn device_rename(app: AppHandle, serial: String, from: String, to: String) -> Result<()> {
    adb::files::rename(&app, &serial, &from, &to).await
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn pull_file(
    app: AppHandle,
    serial: String,
    remote: String,
    local: String,
    id: String,
    name: String,
    total: u64,
    is_dir: bool,
) -> Result<()> {
    adb::transfer::pull(&app, &serial, &remote, &local, &id, &name, total, is_dir).await
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn push_file(
    app: AppHandle,
    serial: String,
    local: String,
    remote: String,
    id: String,
    name: String,
    total: u64,
    is_dir: bool,
) -> Result<()> {
    adb::transfer::push(&app, &serial, &local, &remote, &id, &name, total, is_dir).await
}

#[tauri::command]
pub fn cancel_transfer(app: AppHandle, id: String) {
    adb::transfer::cancel(&app, &id);
}

// ----- Open / preview -----

/// Open a local file with its default macOS application.
#[tauri::command]
pub fn open_local(path: String) -> Result<()> {
    std::process::Command::new("open").arg(&path).spawn()?;
    Ok(())
}

/// Download a device file to a temp folder, then open it with the default app.
#[tauri::command]
pub async fn open_device_file(
    app: AppHandle,
    serial: String,
    remote: String,
    name: String,
) -> Result<()> {
    adb::validate_device_path(&remote)?;
    let dir = std::env::temp_dir().join("freedroid-open");
    std::fs::create_dir_all(&dir)?;
    let local = dir.join(&name);
    let local_str = local.to_string_lossy().to_string();
    adb::run_on(&app, &serial, &["pull", "-a", &remote, &local_str]).await?;
    std::process::Command::new("open").arg(&local).spawn()?;
    Ok(())
}

// ----- Local filesystem (Mac pane) -----

#[tauri::command]
pub fn list_local_dir(path: String) -> Result<Vec<LocalEntry>> {
    local::list_dir(&path)
}

#[tauri::command]
pub fn local_make_dir(path: String) -> Result<()> {
    std::fs::create_dir(&path)?;
    Ok(())
}

#[tauri::command]
pub fn local_rename(from: String, to: String) -> Result<()> {
    std::fs::rename(&from, &to)?;
    Ok(())
}

/// Move local items to the Trash (via Finder) so deletes stay reversible.
#[tauri::command]
pub fn local_trash(paths: Vec<String>) -> Result<()> {
    for p in &paths {
        let script = format!(
            "tell application \"Finder\" to delete POSIX file \"{}\"",
            p.replace('\\', "\\\\").replace('"', "\\\"")
        );
        let status = std::process::Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .status()?;
        if !status.success() {
            return Err(crate::error::Error::Other(format!(
                "could not move {p} to Trash"
            )));
        }
    }
    Ok(())
}

#[tauri::command]
pub fn local_home() -> String {
    local::home_dir()
}

// ----- MTP (connect without USB debugging) -----

#[tauri::command]
pub fn mtp_connect(mtp: State<'_, Mtp>) -> Result<DeviceInfo> {
    mtp.connect().map_err(Error::Other)
}

#[tauri::command]
pub fn mtp_list(mtp: State<'_, Mtp>, storage: u32, parent: u32) -> Result<Vec<MtpEntry>> {
    mtp.list(storage, parent).map_err(Error::Other)
}

#[tauri::command]
pub fn mtp_pull(mtp: State<'_, Mtp>, id: u32, local: String) -> Result<()> {
    mtp.pull(id, local).map_err(Error::Other)
}

#[tauri::command]
pub fn mtp_push(
    mtp: State<'_, Mtp>,
    local: String,
    parent: u32,
    storage: u32,
    name: String,
) -> Result<u32> {
    mtp.push(local, parent, storage, name).map_err(Error::Other)
}

#[tauri::command]
pub fn mtp_mkdir(mtp: State<'_, Mtp>, name: String, parent: u32, storage: u32) -> Result<u32> {
    mtp.mkdir(name, parent, storage).map_err(Error::Other)
}

#[tauri::command]
pub fn mtp_delete(mtp: State<'_, Mtp>, id: u32) -> Result<()> {
    mtp.delete(id).map_err(Error::Other)
}

#[tauri::command]
pub fn mtp_disconnect(mtp: State<'_, Mtp>) -> Result<()> {
    mtp.disconnect().map_err(Error::Other)
}

// ----- Wi-Fi -----

#[tauri::command]
pub async fn wifi_enable_tcpip(app: AppHandle, serial: String) -> Result<String> {
    wifi::enable_tcpip(&app, &serial).await
}

#[tauri::command]
pub async fn wifi_connect(app: AppHandle, address: String) -> Result<String> {
    wifi::connect(&app, &address).await
}

#[tauri::command]
pub async fn wifi_disconnect(app: AppHandle, address: String) -> Result<()> {
    wifi::disconnect(&app, &address).await
}

#[tauri::command]
pub async fn wifi_pair(app: AppHandle, address: String, code: String) -> Result<String> {
    wifi::pair(&app, &address, &code).await
}
