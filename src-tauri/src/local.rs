//! Local (Mac) filesystem listing for the left pane.

use crate::error::Result;
use serde::Serialize;
use std::time::UNIX_EPOCH;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalEntry {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub mtime: i64,
    pub is_dir: bool,
    pub is_symlink: bool,
}

pub fn list_dir(path: &str) -> Result<Vec<LocalEntry>> {
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        let name = entry.file_name().to_string_lossy().to_string();
        // Skip dotfiles to mirror Finder's default view.
        if name.starts_with('.') {
            continue;
        }
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        entries.push(LocalEntry {
            name,
            path: entry.path().to_string_lossy().to_string(),
            size: meta.len(),
            mtime,
            is_dir: meta.is_dir(),
            is_symlink: meta.file_type().is_symlink(),
        });
    }
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(entries)
}

/// The user's home directory as the default starting point.
pub fn home_dir() -> String {
    std::env::var("HOME").unwrap_or_else(|_| "/".to_string())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    pub label: String,
    pub path: String,
}

/// Quick-access locations for the Mac pane: Home, the whole disk, and every
/// mounted volume under /Volumes (USB drives, SD cards, disk images, shares).
pub fn locations() -> Vec<Location> {
    let mut out = vec![
        Location {
            label: "Home".into(),
            path: home_dir(),
        },
        Location {
            label: "Computer".into(),
            path: "/".into(),
        },
    ];
    if let Ok(rd) = std::fs::read_dir("/Volumes") {
        let mut vols: Vec<Location> = rd
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                if name.starts_with('.') {
                    return None;
                }
                Some(Location {
                    label: name.clone(),
                    path: format!("/Volumes/{name}"),
                })
            })
            .collect();
        vols.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()));
        out.extend(vols);
    }
    out
}
