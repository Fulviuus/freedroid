//! Device-side file listing and manipulation via `adb shell`.

use crate::adb::{run_on, shell_quote, validate_device_path};
use crate::error::{Error, Result};
use serde::Serialize;
use tauri::AppHandle;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirEntry {
    pub name: String,
    pub path: String,
    pub size: u64,
    /// Unix epoch seconds.
    pub mtime: i64,
    pub is_dir: bool,
    pub is_symlink: bool,
}

/// List a directory on the device. Uses a single `find ... -exec stat` so we get
/// name/size/mtime/type for every entry in one round-trip.
///
/// `stat -c '%n|%s|%Y|%F'` prints: full-path | size-bytes | mtime-epoch | type.
/// We parse from the right (`rsplitn`) because the trailing three fields never
/// contain `|`, so filenames containing `|` are still handled correctly.
pub async fn list_dir(app: &AppHandle, serial: &str, path: &str) -> Result<Vec<DirEntry>> {
    validate_device_path(path)?;
    // Append a trailing slash so `find` follows a symlinked start point. The
    // storage root /sdcard is itself a symlink (-> /storage/self/primary), and
    // without the slash `find` treats it as a file and lists nothing. A trailing
    // slash is harmless on regular directories. `find` normalises the doubled
    // slash, so the reported paths (and basenames) are unaffected.
    let dir = format!("{}/", path.trim_end_matches('/'));
    let quoted = shell_quote(&dir);
    let cmd = format!(
        "find {quoted} -maxdepth 1 -mindepth 1 -exec stat -c '%n|%s|%Y|%F' {{}} + 2>/dev/null"
    );
    let out = run_on(app, serial, &["shell", &cmd]).await?;
    Ok(parse_stat_lines(&out))
}

fn parse_stat_lines(out: &str) -> Vec<DirEntry> {
    let mut entries = Vec::new();
    for line in out.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        // rsplitn(4) => [type, mtime, size, name(may contain '|')]
        let mut it = line.rsplitn(4, '|');
        let Some(ftype) = it.next() else { continue };
        let Some(mtime_s) = it.next() else { continue };
        let Some(size_s) = it.next() else { continue };
        let Some(full_path) = it.next() else {
            continue;
        };

        let size = size_s.trim().parse::<u64>().unwrap_or(0);
        let mtime = mtime_s.trim().parse::<i64>().unwrap_or(0);
        let is_dir = ftype.contains("directory");
        let is_symlink = ftype.contains("symbolic link");
        let name = full_path
            .rsplit('/')
            .next()
            .unwrap_or(full_path)
            .to_string();

        entries.push(DirEntry {
            name,
            path: full_path.to_string(),
            size,
            mtime,
            is_dir,
            is_symlink,
        });
    }
    // Directories first, then case-insensitive name order.
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    entries
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Volume {
    pub label: String,
    pub path: String,
}

/// List the device's user-accessible storage volumes: internal storage plus any
/// SD card / USB volumes mounted under /storage.
pub async fn list_volumes(app: &AppHandle, serial: &str) -> Result<Vec<Volume>> {
    let mut vols = vec![Volume {
        label: "Internal storage".into(),
        path: "/sdcard".into(),
    }];
    let out = run_on(app, serial, &["shell", "ls /storage 2>/dev/null"])
        .await
        .unwrap_or_default();
    for name in out.split_whitespace() {
        if name.is_empty() || name == "emulated" || name == "self" {
            continue;
        }
        // Volume ids look like "ABCD-1234"; show those as an SD card.
        let is_uuid = name.len() == 9
            && name.as_bytes()[4] == b'-'
            && name.chars().enumerate().all(|(i, c)| i == 4 || c.is_ascii_hexdigit());
        vols.push(Volume {
            label: if is_uuid { "SD card".into() } else { name.to_string() },
            path: format!("/storage/{name}"),
        });
    }
    Ok(vols)
}

pub async fn make_dir(app: &AppHandle, serial: &str, path: &str) -> Result<()> {
    validate_device_path(path)?;
    let cmd = format!("mkdir -p {}", shell_quote(path));
    run_on(app, serial, &["shell", &cmd]).await?;
    Ok(())
}

/// Validate a path is safe to delete: inside device storage and not a root.
fn check_deletable(path: &str) -> Result<()> {
    validate_device_path(path)?;
    if path == "/sdcard" || path == "/storage" || path.trim_end_matches('/').matches('/').count() < 2
    {
        return Err(Error::InvalidPath(format!(
            "refusing to delete a storage root: {path}"
        )));
    }
    Ok(())
}

pub async fn remove(app: &AppHandle, serial: &str, path: &str) -> Result<()> {
    check_deletable(path)?;
    let cmd = format!("rm -rf {}", shell_quote(path));
    run_on(app, serial, &["shell", &cmd]).await?;
    Ok(())
}

/// Delete many paths in a single `rm -rf` so large selections are one round-trip.
pub async fn remove_many(app: &AppHandle, serial: &str, paths: &[String]) -> Result<()> {
    if paths.is_empty() {
        return Ok(());
    }
    let mut quoted = Vec::with_capacity(paths.len());
    for p in paths {
        check_deletable(p)?;
        quoted.push(shell_quote(p));
    }
    let cmd = format!("rm -rf {}", quoted.join(" "));
    run_on(app, serial, &["shell", &cmd]).await?;
    Ok(())
}

pub async fn rename(app: &AppHandle, serial: &str, from: &str, to: &str) -> Result<()> {
    validate_device_path(from)?;
    validate_device_path(to)?;
    let cmd = format!("mv {} {}", shell_quote(from), shell_quote(to));
    run_on(app, serial, &["shell", &cmd]).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_stat_output() {
        let out = "/sdcard/DCIM|4096|1700000000|directory\n\
                   /sdcard/song|name.mp3|5242880|1700000100|regular file\n\
                   /sdcard/link|1024|1700000200|symbolic link\n";
        let e = parse_stat_lines(out);
        assert_eq!(e.len(), 3);
        // sorted: directory first
        assert_eq!(e[0].name, "DCIM");
        assert!(e[0].is_dir);
        // filename containing '|' preserved
        let mp3 = e.iter().find(|x| x.name == "song|name.mp3").unwrap();
        assert_eq!(mp3.size, 5242880);
        assert_eq!(mp3.mtime, 1700000100);
        let link = e.iter().find(|x| x.name == "link").unwrap();
        assert!(link.is_symlink);
    }
}
