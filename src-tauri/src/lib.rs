mod adb;
mod commands;
mod error;
mod fuse;
mod local;

use tauri::{Manager, RunEvent, WindowEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .manage(fuse::FuseState::default())
        .invoke_handler(tauri::generate_handler![
            commands::adb_version,
            commands::list_devices,
            commands::list_device_dir,
            commands::device_make_dir,
            commands::device_remove,
            commands::device_rename,
            commands::pull_file,
            commands::push_file,
            commands::list_local_dir,
            commands::local_home,
            commands::wifi_enable_tcpip,
            commands::wifi_connect,
            commands::wifi_disconnect,
            commands::wifi_pair,
            commands::fuse_available,
            commands::mount_device,
            commands::unmount_device,
            commands::current_mountpoint,
        ])
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|app, event| {
            // On the last window closing, stop the adb server so we don't leave
            // an orphaned background process behind.
            if let RunEvent::WindowEvent {
                event: WindowEvent::Destroyed,
                ..
            } = event
            {
                let _ = fuse::unmount(&app.state::<fuse::FuseState>());
                let app = app.clone();
                tauri::async_runtime::block_on(async move {
                    adb::kill_server(&app).await;
                });
            }
        });
}
