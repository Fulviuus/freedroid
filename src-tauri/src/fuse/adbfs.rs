//! A FUSE filesystem that proxies an Android device's storage over adb.
//!
//! Design (see the plan's M6 notes):
//!  * Metadata comes from a short-TTL cache of directory listings, so `lookup`
//!    and `getattr` don't shell out to adb on every call.
//!  * File *content* is fetched whole on `open` (`adb pull` into a temp cache
//!    dir) and served locally; writes go to the temp copy and are pushed back
//!    on `release` if the file is dirty. This keeps Finder responsive — it
//!    reads whole files for copies and Quick Look previews anyway.
//!  * adb's single server serializes operations; FUSE callbacks are synchronous
//!    so we drive the existing async adb wrapper with `block_on`.

use crate::adb;
use fuser::{
    FileAttr, FileType, Filesystem, ReplyAttr, ReplyCreate, ReplyData, ReplyDirectory,
    ReplyEmpty, ReplyEntry, ReplyOpen, ReplyWrite, Request,
};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::AppHandle;

const TTL: Duration = Duration::from_secs(2);
const CACHE_TTL: Duration = Duration::from_secs(5);
const ROOT_INO: u64 = 1;
const BLOCK_SIZE: u32 = 512;

struct Node {
    path: String,
    is_dir: bool,
    size: u64,
    mtime: i64,
}

struct OpenHandle {
    temp: PathBuf,
    dirty: bool,
    refcount: u32,
}

pub struct AdbFs {
    app: AppHandle,
    serial: String,
    root: String,
    cache_dir: PathBuf,
    inodes: HashMap<u64, Node>,
    by_path: HashMap<String, u64>,
    /// dir inode -> (fetched_at, child inodes in listing order)
    dir_cache: HashMap<u64, (Instant, Vec<u64>)>,
    open: HashMap<u64, OpenHandle>,
    next_ino: u64,
    uid: u32,
    gid: u32,
}

impl AdbFs {
    pub fn new(app: AppHandle, serial: String, root: String, cache_dir: PathBuf) -> Self {
        let mut inodes = HashMap::new();
        let mut by_path = HashMap::new();
        inodes.insert(
            ROOT_INO,
            Node {
                path: root.clone(),
                is_dir: true,
                size: 0,
                mtime: now_secs(),
            },
        );
        by_path.insert(root.clone(), ROOT_INO);
        AdbFs {
            app,
            serial,
            root,
            cache_dir,
            inodes,
            by_path,
            dir_cache: HashMap::new(),
            open: HashMap::new(),
            next_ino: 2,
            uid: unsafe { libc::getuid() },
            gid: unsafe { libc::getgid() },
        }
    }

    fn attr(&self, ino: u64) -> Option<FileAttr> {
        let n = self.inodes.get(&ino)?;
        Some(self.make_attr(ino, n.is_dir, n.size, n.mtime))
    }

    fn make_attr(&self, ino: u64, is_dir: bool, size: u64, mtime: i64) -> FileAttr {
        let t = epoch(mtime);
        FileAttr {
            ino,
            size,
            blocks: size.div_ceil(BLOCK_SIZE as u64),
            atime: t,
            mtime: t,
            ctime: t,
            crtime: t,
            kind: if is_dir {
                FileType::Directory
            } else {
                FileType::RegularFile
            },
            perm: if is_dir { 0o755 } else { 0o644 },
            nlink: if is_dir { 2 } else { 1 },
            uid: self.uid,
            gid: self.gid,
            rdev: 0,
            blksize: BLOCK_SIZE,
            flags: 0,
        }
    }

    fn intern(&mut self, path: String, is_dir: bool, size: u64, mtime: i64) -> u64 {
        if let Some(&ino) = self.by_path.get(&path) {
            if let Some(n) = self.inodes.get_mut(&ino) {
                n.is_dir = is_dir;
                n.size = size;
                n.mtime = mtime;
            }
            return ino;
        }
        let ino = self.next_ino;
        self.next_ino += 1;
        self.by_path.insert(path.clone(), ino);
        self.inodes.insert(
            ino,
            Node {
                path,
                is_dir,
                size,
                mtime,
            },
        );
        ino
    }

    /// Ensure the directory `ino`'s listing is cached & fresh; return child inodes.
    fn ensure_listed(&mut self, ino: u64) -> Result<Vec<u64>, i32> {
        if let Some((at, kids)) = self.dir_cache.get(&ino) {
            if at.elapsed() < CACHE_TTL {
                return Ok(kids.clone());
            }
        }
        let dir_path = self.inodes.get(&ino).ok_or(libc::ENOENT)?.path.clone();
        let entries = block_on(adb::files::list_dir(&self.app, &self.serial, &dir_path))
            .map_err(|_| libc::EIO)?;
        let mut kids = Vec::with_capacity(entries.len());
        for e in entries {
            let cino = self.intern(e.path, e.is_dir, e.size, e.mtime);
            kids.push(cino);
        }
        self.dir_cache.insert(ino, (Instant::now(), kids.clone()));
        Ok(kids)
    }

    fn invalidate(&mut self, ino: u64) {
        self.dir_cache.remove(&ino);
    }

    fn child_path(&self, parent: u64, name: &OsStr) -> Option<String> {
        let p = &self.inodes.get(&parent)?.path;
        Some(format!("{}/{}", p.trim_end_matches('/'), name.to_string_lossy()))
    }

    fn temp_for(&self, ino: u64) -> PathBuf {
        self.cache_dir.join(format!("ino-{ino}"))
    }
}

impl Filesystem for AdbFs {
    fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
        let kids = match self.ensure_listed(parent) {
            Ok(k) => k,
            Err(e) => return reply.error(e),
        };
        let target = name.to_string_lossy();
        for ino in kids {
            if let Some(n) = self.inodes.get(&ino) {
                if n.path.rsplit('/').next() == Some(target.as_ref()) {
                    if let Some(a) = self.attr(ino) {
                        return reply.entry(&TTL, &a, 0);
                    }
                }
            }
        }
        reply.error(libc::ENOENT);
    }

    fn getattr(&mut self, _req: &Request, ino: u64, _fh: Option<u64>, reply: ReplyAttr) {
        match self.attr(ino) {
            Some(a) => reply.attr(&TTL, &a),
            None => reply.error(libc::ENOENT),
        }
    }

    fn readdir(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        mut reply: ReplyDirectory,
    ) {
        let kids = match self.ensure_listed(ino) {
            Ok(k) => k,
            Err(e) => return reply.error(e),
        };
        let mut entries: Vec<(u64, FileType, String)> = vec![
            (ino, FileType::Directory, ".".into()),
            (ROOT_INO, FileType::Directory, "..".into()),
        ];
        for cino in kids {
            if let Some(n) = self.inodes.get(&cino) {
                let name = n.path.rsplit('/').next().unwrap_or("").to_string();
                let ft = if n.is_dir {
                    FileType::Directory
                } else {
                    FileType::RegularFile
                };
                entries.push((cino, ft, name));
            }
        }
        for (i, (cino, ft, name)) in entries.into_iter().enumerate().skip(offset as usize) {
            if reply.add(cino, (i + 1) as i64, ft, name) {
                break; // buffer full
            }
        }
        reply.ok();
    }

    fn open(&mut self, _req: &Request, ino: u64, _flags: i32, reply: ReplyOpen) {
        let Some(node) = self.inodes.get(&ino) else {
            return reply.error(libc::ENOENT);
        };
        if node.is_dir {
            return reply.error(libc::EISDIR);
        }
        let remote = node.path.clone();
        let temp = self.temp_for(ino);
        // Refcount existing open handle, else pull a fresh copy.
        if let Some(h) = self.open.get_mut(&ino) {
            h.refcount += 1;
            return reply.opened(0, 0);
        }
        if let Some(parent) = temp.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(_) = block_on(adb::files::pull_to(
            &self.app,
            &self.serial,
            &remote,
            &temp.to_string_lossy(),
        )) {
            return reply.error(libc::EIO);
        }
        self.open.insert(
            ino,
            OpenHandle {
                temp,
                dirty: false,
                refcount: 1,
            },
        );
        reply.opened(0, 0);
    }

    fn read(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        size: u32,
        _flags: i32,
        _lock: Option<u64>,
        reply: ReplyData,
    ) {
        let Some(h) = self.open.get(&ino) else {
            return reply.error(libc::EBADF);
        };
        use std::io::{Read, Seek, SeekFrom};
        let mut f = match std::fs::File::open(&h.temp) {
            Ok(f) => f,
            Err(_) => return reply.error(libc::EIO),
        };
        if f.seek(SeekFrom::Start(offset as u64)).is_err() {
            return reply.error(libc::EIO);
        }
        let mut buf = vec![0u8; size as usize];
        match f.read(&mut buf) {
            Ok(n) => reply.data(&buf[..n]),
            Err(_) => reply.error(libc::EIO),
        }
    }

    fn write(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        data: &[u8],
        _wf: u32,
        _flags: i32,
        _lock: Option<u64>,
        reply: ReplyWrite,
    ) {
        let Some(h) = self.open.get_mut(&ino) else {
            return reply.error(libc::EBADF);
        };
        use std::io::{Seek, SeekFrom, Write};
        let mut f = match std::fs::OpenOptions::new().write(true).open(&h.temp) {
            Ok(f) => f,
            Err(_) => return reply.error(libc::EIO),
        };
        if f.seek(SeekFrom::Start(offset as u64)).is_err() {
            return reply.error(libc::EIO);
        }
        match f.write_all(data) {
            Ok(_) => {
                h.dirty = true;
                let new_size = (offset as u64) + data.len() as u64;
                if let Some(n) = self.inodes.get_mut(&ino) {
                    if new_size > n.size {
                        n.size = new_size;
                    }
                }
                reply.written(data.len() as u32);
            }
            Err(_) => reply.error(libc::EIO),
        }
    }

    fn create(
        &mut self,
        _req: &Request,
        parent: u64,
        name: &OsStr,
        _mode: u32,
        _umask: u32,
        _flags: i32,
        reply: ReplyCreate,
    ) {
        let Some(path) = self.child_path(parent, name) else {
            return reply.error(libc::ENOENT);
        };
        let ino = self.intern(path, false, 0, now_secs());
        let temp = self.temp_for(ino);
        if let Some(p) = temp.parent() {
            let _ = std::fs::create_dir_all(p);
        }
        if std::fs::File::create(&temp).is_err() {
            return reply.error(libc::EIO);
        }
        self.open.insert(
            ino,
            OpenHandle {
                temp,
                dirty: true, // empty file must be pushed on release
                refcount: 1,
            },
        );
        self.invalidate(parent);
        let attr = self.make_attr(ino, false, 0, now_secs());
        reply.created(&TTL, &attr, 0, 0, 0);
    }

    fn release(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        _flags: i32,
        _lock: Option<u64>,
        _flush: bool,
        reply: ReplyEmpty,
    ) {
        let push_info = {
            let Some(h) = self.open.get_mut(&ino) else {
                return reply.ok();
            };
            h.refcount = h.refcount.saturating_sub(1);
            if h.refcount == 0 && h.dirty {
                Some(h.temp.clone())
            } else {
                None
            }
        };
        if let Some(temp) = push_info {
            let remote = self.inodes.get(&ino).map(|n| n.path.clone());
            if let Some(remote) = remote {
                let _ = block_on(adb::files::push_from(
                    &self.app,
                    &self.serial,
                    &temp.to_string_lossy(),
                    &remote,
                ));
                // Invalidate the parent dir cache so the new size shows up.
                if let Some(parent) = parent_path_ino(self, ino) {
                    self.invalidate(parent);
                }
            }
        }
        // Drop the handle once fully closed.
        if self.open.get(&ino).map(|h| h.refcount) == Some(0) {
            self.open.remove(&ino);
        }
        reply.ok();
    }

    fn mkdir(
        &mut self,
        _req: &Request,
        parent: u64,
        name: &OsStr,
        _mode: u32,
        _umask: u32,
        reply: ReplyEntry,
    ) {
        let Some(path) = self.child_path(parent, name) else {
            return reply.error(libc::ENOENT);
        };
        if block_on(adb::files::make_dir(&self.app, &self.serial, &path)).is_err() {
            return reply.error(libc::EIO);
        }
        let ino = self.intern(path, true, 0, now_secs());
        self.invalidate(parent);
        let attr = self.make_attr(ino, true, 0, now_secs());
        reply.entry(&TTL, &attr, 0);
    }

    fn unlink(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEmpty) {
        self.remove_entry(parent, name, reply);
    }

    fn rmdir(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEmpty) {
        self.remove_entry(parent, name, reply);
    }

    fn rename(
        &mut self,
        _req: &Request,
        parent: u64,
        name: &OsStr,
        newparent: u64,
        newname: &OsStr,
        _flags: u32,
        reply: ReplyEmpty,
    ) {
        let (Some(from), Some(to)) = (
            self.child_path(parent, name),
            self.child_path(newparent, newname),
        ) else {
            return reply.error(libc::ENOENT);
        };
        if block_on(adb::files::rename(&self.app, &self.serial, &from, &to)).is_err() {
            return reply.error(libc::EIO);
        }
        self.by_path.remove(&from);
        self.invalidate(parent);
        self.invalidate(newparent);
        reply.ok();
    }

    fn setattr(
        &mut self,
        _req: &Request,
        ino: u64,
        _mode: Option<u32>,
        _uid: Option<u32>,
        _gid: Option<u32>,
        size: Option<u64>,
        _atime: Option<fuser::TimeOrNow>,
        _mtime: Option<fuser::TimeOrNow>,
        _ctime: Option<SystemTime>,
        _fh: Option<u64>,
        _crtime: Option<SystemTime>,
        _chgtime: Option<SystemTime>,
        _bkuptime: Option<SystemTime>,
        _flags: Option<u32>,
        reply: ReplyAttr,
    ) {
        // Honor truncation (editors do open→ftruncate→write→close).
        if let Some(sz) = size {
            if let Some(h) = self.open.get_mut(&ino) {
                use std::fs::OpenOptions;
                if let Ok(f) = OpenOptions::new().write(true).open(&h.temp) {
                    let _ = f.set_len(sz);
                    h.dirty = true;
                }
            }
            if let Some(n) = self.inodes.get_mut(&ino) {
                n.size = sz;
            }
        }
        match self.attr(ino) {
            Some(a) => reply.attr(&TTL, &a),
            None => reply.error(libc::ENOENT),
        }
    }
}

impl AdbFs {
    fn remove_entry(&mut self, parent: u64, name: &OsStr, reply: ReplyEmpty) {
        let Some(path) = self.child_path(parent, name) else {
            return reply.error(libc::ENOENT);
        };
        if block_on(adb::files::remove(&self.app, &self.serial, &path)).is_err() {
            return reply.error(libc::EIO);
        }
        self.by_path.remove(&path);
        self.invalidate(parent);
        reply.ok();
    }
}

fn parent_path_ino(fs: &AdbFs, ino: u64) -> Option<u64> {
    let path = &fs.inodes.get(&ino)?.path;
    let parent = path.rsplitn(2, '/').nth(1)?;
    let parent = if parent.is_empty() { &fs.root } else { parent };
    fs.by_path.get(parent).copied()
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn epoch(secs: i64) -> SystemTime {
    if secs >= 0 {
        UNIX_EPOCH + Duration::from_secs(secs as u64)
    } else {
        UNIX_EPOCH
    }
}

/// Run an async adb future to completion from a synchronous FUSE callback.
/// The fuser session runs on its own std thread (not a tokio worker), so this
/// blocks only that thread.
fn block_on<F: std::future::Future>(fut: F) -> F::Output {
    tauri::async_runtime::block_on(fut)
}
