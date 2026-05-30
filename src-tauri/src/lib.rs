mod adb;
mod commands;
mod error;
mod local;
pub mod mtp;

use tauri::{Manager, RunEvent, WindowEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .manage(adb::transfer::TransferRegistry::default())
        .manage(mtp::Mtp::default())
        .invoke_handler(tauri::generate_handler![
            commands::adb_version,
            commands::list_devices,
            commands::list_device_dir,
            commands::list_volumes,
            commands::device_make_dir,
            commands::device_remove,
            commands::device_remove_many,
            commands::device_rename,
            commands::pull_file,
            commands::push_file,
            commands::cancel_transfer,
            commands::open_local,
            commands::open_device_file,
            commands::list_local_dir,
            commands::local_make_dir,
            commands::local_rename,
            commands::local_trash,
            commands::local_home,
            commands::wifi_enable_tcpip,
            commands::wifi_connect,
            commands::wifi_disconnect,
            commands::wifi_pair,
            commands::mtp_connect,
            commands::mtp_list,
            commands::mtp_pull,
            commands::mtp_push,
            commands::mtp_mkdir,
            commands::mtp_delete,
            commands::mtp_disconnect,
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
                // Release the MTP device so its USB interface isn't left claimed.
                let _ = app.state::<mtp::Mtp>().disconnect();
                let app = app.clone();
                tauri::async_runtime::block_on(async move {
                    adb::kill_server(&app).await;
                });
            }
        });
}
