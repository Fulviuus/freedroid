//! File transfers (push/pull) with live progress.
//!
//! adb only prints its `[ NN%]` progress bar to an interactive terminal; when
//! its output is piped (as it is here) it stays silent until the final summary.
//! So instead of scraping adb's output we measure progress directly: poll the
//! growing destination file's size against the known source size.
//!
//! - pull: stat the local destination file (free, no adb).
//! - push: `adb shell stat -c %s` the remote destination (lightweight; runs fine
//!   concurrently with the push on adb's multiplexed server).
//!
//! Directories (and zero-byte / unknown-size sources) report as indeterminate.

use crate::adb::{run_on, shell_quote, validate_device_path};
use crate::error::{Error, Result};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub id: String,
    pub percent: u8,
    pub direction: String,
    pub name: String,
    /// True when we can't compute a percentage (folders / unknown size).
    pub indeterminate: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Done {
    pub id: String,
    pub success: bool,
    pub error: Option<String>,
}

/// Where to look to measure how far a transfer has progressed.
enum Probe {
    Local(String),
    Remote { serial: String, path: String },
    None,
}

fn join(dir: &str, name: &str) -> String {
    format!("{}/{}", dir.trim_end_matches('/'), name)
}

/// Pull a file/dir from the device into the local destination directory.
#[allow(clippy::too_many_arguments)]
pub async fn pull(
    app: &AppHandle,
    serial: &str,
    remote: &str,
    local_dir: &str,
    id: &str,
    name: &str,
    total: u64,
    is_dir: bool,
) -> Result<()> {
    validate_device_path(remote)?;
    let probe = if is_dir || total == 0 {
        Probe::None
    } else {
        Probe::Local(join(local_dir, name))
    };
    run_transfer(
        app,
        id,
        name,
        "pull",
        &["-s", serial, "pull", "-a", remote, local_dir],
        probe,
        total,
    )
    .await
}

/// Push a local file/dir to the device destination directory.
#[allow(clippy::too_many_arguments)]
pub async fn push(
    app: &AppHandle,
    serial: &str,
    local: &str,
    remote_dir: &str,
    id: &str,
    name: &str,
    total: u64,
    is_dir: bool,
) -> Result<()> {
    validate_device_path(remote_dir)?;
    let probe = if is_dir || total == 0 {
        Probe::None
    } else {
        Probe::Remote {
            serial: serial.to_string(),
            path: join(remote_dir, name),
        }
    };
    run_transfer(
        app,
        id,
        name,
        "push",
        &["-s", serial, "push", local, remote_dir],
        probe,
        total,
    )
    .await
}

async fn sample(app: &AppHandle, probe: &Probe) -> Option<u64> {
    match probe {
        Probe::Local(p) => std::fs::metadata(p).ok().map(|m| m.len()),
        Probe::Remote { serial, path } => {
            let cmd = format!("stat -c %s {} 2>/dev/null", shell_quote(path));
            let out = run_on(app, serial, &["shell", &cmd]).await.ok()?;
            out.trim().parse::<u64>().ok()
        }
        Probe::None => None,
    }
}

fn emit_progress(app: &AppHandle, id: &str, name: &str, dir: &str, pct: u8, indeterminate: bool) {
    let _ = app.emit(
        "transfer://progress",
        Progress {
            id: id.to_string(),
            percent: pct,
            direction: dir.to_string(),
            name: name.to_string(),
            indeterminate,
        },
    );
}

async fn run_transfer(
    app: &AppHandle,
    id: &str,
    name: &str,
    direction: &str,
    args: &[&str],
    probe: Probe,
    total: u64,
) -> Result<()> {
    let cmd = app.shell().sidecar("adb")?;
    let (mut rx, _child) = cmd.args(args).spawn()?;

    // Background sampler drives the percentage by watching the destination size.
    let stop = Arc::new(AtomicBool::new(false));
    let poll = if total > 0 && !matches!(probe, Probe::None) {
        let app = app.clone();
        let stop = stop.clone();
        let id = id.to_string();
        let name = name.to_string();
        let direction = direction.to_string();
        Some(tauri::async_runtime::spawn(async move {
            let mut last = 0u8;
            while !stop.load(Ordering::Relaxed) {
                if let Some(sz) = sample(&app, &probe).await {
                    let pct = (((sz.min(total) as f64) / total as f64) * 100.0) as u8;
                    let pct = pct.min(99);
                    if pct != last {
                        last = pct;
                        emit_progress(&app, &id, &name, &direction, pct, false);
                    }
                }
                tokio::time::sleep(Duration::from_millis(400)).await;
            }
        }))
    } else {
        // No measurable size — tell the UI to show an indeterminate bar.
        emit_progress(app, id, name, direction, 0, true);
        None
    };

    let mut stderr_buf = String::new();
    let mut exit_ok = false;
    while let Some(event) = rx.recv().await {
        match event {
            CommandEvent::Stderr(b) => stderr_buf.push_str(&String::from_utf8_lossy(&b)),
            CommandEvent::Terminated(p) => exit_ok = p.code == Some(0),
            _ => {}
        }
    }

    // Stop the sampler before emitting the terminal state.
    stop.store(true, Ordering::Relaxed);
    if let Some(h) = poll {
        h.abort();
    }

    let result = if exit_ok {
        emit_progress(app, id, name, direction, 100, false);
        Ok(())
    } else {
        Err(Error::Adb(if stderr_buf.trim().is_empty() {
            "transfer failed".into()
        } else {
            stderr_buf.trim().to_string()
        }))
    };

    let _ = app.emit(
        "transfer://done",
        Done {
            id: id.to_string(),
            success: result.is_ok(),
            error: result.as_ref().err().map(|e| e.to_string()),
        },
    );

    result
}
