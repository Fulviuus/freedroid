//! File transfers (push/pull) with live progress.
//!
//! adb prints per-file progress like `[ 45%] /sdcard/foo` while transferring.
//! We spawn the sidecar, scan stdout/stderr for those markers, and emit a
//! `transfer://progress` event the frontend listens on. A `transfer://done`
//! event fires on completion (success or failure).

use crate::adb::validate_device_path;
use crate::error::{Error, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;

static PCT_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\[\s*(\d+)%\]").unwrap());

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub id: String,
    pub percent: u8,
    pub direction: String,
    pub name: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Done {
    pub id: String,
    pub success: bool,
    pub error: Option<String>,
}

/// Pull a file/dir from the device to the local filesystem.
pub async fn pull(
    app: &AppHandle,
    serial: &str,
    remote: &str,
    local: &str,
    id: &str,
    name: &str,
) -> Result<()> {
    validate_device_path(remote)?;
    run_transfer(app, id, name, "pull", &["-s", serial, "pull", "-a", remote, local]).await
}

/// Push a local file/dir to the device.
pub async fn push(
    app: &AppHandle,
    serial: &str,
    local: &str,
    remote: &str,
    id: &str,
    name: &str,
) -> Result<()> {
    validate_device_path(remote)?;
    run_transfer(app, id, name, "push", &["-s", serial, "push", local, remote]).await
}

async fn run_transfer(
    app: &AppHandle,
    id: &str,
    name: &str,
    direction: &str,
    args: &[&str],
) -> Result<()> {
    let cmd = app.shell().sidecar("adb")?;
    let (mut rx, _child) = cmd.args(args).spawn()?;

    let mut last_pct: u8 = 0;
    let mut stderr_buf = String::new();
    let mut exit_ok = false;

    while let Some(event) = rx.recv().await {
        let (text, is_stderr) = match event {
            CommandEvent::Stdout(bytes) => (String::from_utf8_lossy(&bytes).into_owned(), false),
            CommandEvent::Stderr(bytes) => (String::from_utf8_lossy(&bytes).into_owned(), true),
            CommandEvent::Terminated(payload) => {
                exit_ok = payload.code == Some(0);
                continue;
            }
            _ => continue,
        };

        if is_stderr {
            stderr_buf.push_str(&text);
        }
        if let Some(c) = PCT_RE.captures_iter(&text).last() {
            if let Ok(p) = c[1].parse::<u8>() {
                if p != last_pct {
                    last_pct = p;
                    let _ = app.emit(
                        "transfer://progress",
                        Progress {
                            id: id.to_string(),
                            percent: p,
                            direction: direction.to_string(),
                            name: name.to_string(),
                        },
                    );
                }
            }
        }
    }

    let result = if exit_ok {
        let _ = app.emit(
            "transfer://progress",
            Progress {
                id: id.to_string(),
                percent: 100,
                direction: direction.to_string(),
                name: name.to_string(),
            },
        );
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
