//! Tauri command surface — the bridge invoked from the Svelte frontend.

use crate::adb::{self, devices::Device, files::DirEntry, wifi};
use crate::error::Result;
use crate::local::{self, LocalEntry};
use tauri::AppHandle;

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
pub async fn device_make_dir(app: AppHandle, serial: String, path: String) -> Result<()> {
    adb::files::make_dir(&app, &serial, &path).await
}

#[tauri::command]
pub async fn device_remove(app: AppHandle, serial: String, path: String) -> Result<()> {
    adb::files::remove(&app, &serial, &path).await
}

#[tauri::command]
pub async fn device_rename(app: AppHandle, serial: String, from: String, to: String) -> Result<()> {
    adb::files::rename(&app, &serial, &from, &to).await
}

#[tauri::command]
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

// ----- Local filesystem (Mac pane) -----

#[tauri::command]
pub fn list_local_dir(path: String) -> Result<Vec<LocalEntry>> {
    local::list_dir(&path)
}

#[tauri::command]
pub fn local_home() -> String {
    local::home_dir()
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
