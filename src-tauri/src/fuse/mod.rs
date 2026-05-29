//! Finder-mount control. Mounts an Android device as a FUSE volume (via fuse-t)
//! so it appears in Finder. Compiled only with `--features fuse`; otherwise the
//! commands return a friendly "not available" error so the rest of the app
//! builds and runs without fuse-t installed.

use crate::error::{Error, Result};
use std::sync::Mutex;
use tauri::AppHandle;

#[cfg(feature = "fuse")]
mod adbfs;

/// Managed Tauri state holding the active mount session, if any.
#[derive(Default)]
pub struct FuseState {
    #[cfg(feature = "fuse")]
    session: Mutex<Option<fuser::BackgroundSession>>,
    mountpoint: Mutex<Option<String>>,
}

/// Whether FUSE mounting is usable on this machine (built with the feature and
/// fuse-t installed).
pub fn available() -> bool {
    #[cfg(feature = "fuse")]
    {
        ["/usr/local/lib/libfuse-t.dylib", "/usr/local/lib/libfuse-t.2.dylib"]
            .iter()
            .any(|p| std::path::Path::new(p).exists())
    }
    #[cfg(not(feature = "fuse"))]
    {
        false
    }
}

#[cfg(feature = "fuse")]
pub fn mount(
    app: &AppHandle,
    state: &FuseState,
    serial: &str,
    device_name: &str,
    root: &str,
) -> Result<String> {
    use fuser::MountOption;

    let mut guard = state.session.lock().unwrap();
    if guard.is_some() {
        return Err(Error::Other("a device is already mounted".into()));
    }
    if !available() {
        return Err(Error::Other(
            "fuse-t is not installed — see https://www.fuse-t.org".into(),
        ));
    }

    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    let mountpoint = std::path::Path::new(&home)
        .join("Freedroid")
        .join(sanitize(device_name));
    // The mountpoint must exist and be empty.
    std::fs::create_dir_all(&mountpoint)?;

    let cache_dir = std::path::Path::new(&home)
        .join("Library/Caches/app.freedroid.desktop")
        .join(sanitize(serial));
    std::fs::create_dir_all(&cache_dir)?;

    let fs = adbfs::AdbFs::new(app.clone(), serial.to_string(), root.to_string(), cache_dir);
    let opts = vec![
        MountOption::FSName("freedroid".into()),
        MountOption::CUSTOM(format!("volname={device_name}")),
        MountOption::CUSTOM("local".into()),
        MountOption::RW,
    ];

    let session = fuser::spawn_mount2(fs, &mountpoint, &opts)
        .map_err(|e| Error::Other(format!("mount failed: {e}")))?;

    let mp = mountpoint.to_string_lossy().to_string();
    *guard = Some(session);
    *state.mountpoint.lock().unwrap() = Some(mp.clone());
    Ok(mp)
}

#[cfg(not(feature = "fuse"))]
pub fn mount(
    _app: &AppHandle,
    _state: &FuseState,
    _serial: &str,
    _device_name: &str,
    _root: &str,
) -> Result<String> {
    Err(Error::Other(
        "This build was compiled without Finder-mount support (rebuild with --features fuse).".into(),
    ))
}

pub fn unmount(state: &FuseState) -> Result<()> {
    #[cfg(feature = "fuse")]
    {
        // Dropping the BackgroundSession unmounts and joins the worker thread.
        let session = state.session.lock().unwrap().take();
        drop(session);
    }
    *state.mountpoint.lock().unwrap() = None;
    Ok(())
}

pub fn mountpoint(state: &FuseState) -> Option<String> {
    state.mountpoint.lock().unwrap().clone()
}

#[cfg(feature = "fuse")]
fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| if c == '/' || c == ':' { '_' } else { c })
        .collect()
}
