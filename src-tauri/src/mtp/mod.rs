//! Native MTP backend via libmtp (optional `mtp` feature).
//!
//! MTP addresses objects by numeric ids (storage id + object id), not paths, so
//! this layer is id-based; the frontend maps paths to ids for the UI.
//!
//! libmtp device handles are NOT thread-safe, so the device lives on a single
//! dedicated worker thread (`Mtp`) and all access goes through a channel. The
//! actual FFI is behind the `mtp` feature; without it a stub returns a clear
//! error so the command surface and UI still build (and CI stays install-free).

#![allow(non_snake_case)]

use serde::Serialize;
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use tauri::AppHandle;

/// Context for emitting live progress during an MTP transfer (matches the adb
/// path's `transfer://progress` events so the frontend handles both uniformly).
pub struct ProgressInfo {
    pub app: AppHandle,
    pub id: String,
    pub name: String,
    pub direction: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Storage {
    pub id: u32,
    pub description: String,
    pub max_capacity: u64,
    pub free_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: u32,
    pub name: String,
    pub size: u64,
    pub mtime: i64,
    pub is_dir: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub name: String,
    pub storages: Vec<Storage>,
}

// ===================== feature = "mtp": real libmtp FFI =====================
#[cfg(feature = "mtp")]
mod imp {
    use super::{DeviceInfo, Entry, ProgressInfo, Storage};
    use crate::adb::transfer::Progress;
    use std::cell::Cell;
    use std::ffi::{CStr, CString};
    use std::os::raw::{c_char, c_int, c_void};
    use std::ptr;
    use std::time::Instant;
    use tauri::Emitter;

    type ProgressFn = extern "C" fn(u64, u64, *const c_void) -> c_int;

    struct ProgressCtx {
        info: ProgressInfo,
        last_pct: Cell<u8>,
        last_sent: Cell<u64>,
        last_t: Cell<Instant>,
    }

    /// libmtp progress callback. Throttled to once per integer percent; computes
    /// throughput + ETA over the interval and emits a `transfer://progress` event.
    extern "C" fn progress_cb(sent: u64, total: u64, data: *const c_void) -> c_int {
        if data.is_null() || total == 0 {
            return 0;
        }
        let ctx = unsafe { &*(data as *const ProgressCtx) };
        let pct = (((sent.min(total)) as f64 / total as f64) * 100.0) as u8;
        let pct = pct.min(99); // 100 comes from the frontend on completion
        if pct == ctx.last_pct.get() {
            return 0;
        }
        let now = Instant::now();
        let dt = now.duration_since(ctx.last_t.get()).as_secs_f64();
        let bps = if dt > 0.0 && sent >= ctx.last_sent.get() {
            ((sent - ctx.last_sent.get()) as f64 / dt) as u64
        } else {
            0
        };
        let eta = if bps > 0 {
            total.saturating_sub(sent) / bps
        } else {
            0
        };
        ctx.last_pct.set(pct);
        ctx.last_sent.set(sent);
        ctx.last_t.set(now);
        let _ = ctx.info.app.emit(
            "transfer://progress",
            Progress {
                id: ctx.info.id.clone(),
                percent: pct,
                direction: ctx.info.direction.to_string(),
                name: ctx.info.name.clone(),
                indeterminate: false,
                bytes_per_sec: bps,
                eta_secs: eta,
            },
        );
        0
    }

    fn ctx_for(info: &Option<ProgressInfo>) -> Option<Box<ProgressCtx>> {
        info.as_ref().map(|i| {
            Box::new(ProgressCtx {
                info: ProgressInfo {
                    app: i.app.clone(),
                    id: i.id.clone(),
                    name: i.name.clone(),
                    direction: i.direction,
                },
                last_pct: Cell::new(255),
                last_sent: Cell::new(0),
                last_t: Cell::new(Instant::now()),
            })
        })
    }

    #[repr(C)]
    struct DeviceEntry {
        vendor: *mut c_char,
        vendor_id: u16,
        product: *mut c_char,
        product_id: u16,
        device_flags: u32,
    }
    #[repr(C)]
    struct RawDevice {
        device_entry: DeviceEntry,
        bus_location: u32,
        devnum: u8,
    }
    #[repr(C)]
    struct MtpFile {
        item_id: u32,
        parent_id: u32,
        storage_id: u32,
        filename: *mut c_char,
        filesize: u64,
        modificationdate: i64,
        filetype: c_int,
        next: *mut MtpFile,
    }
    #[repr(C)]
    struct DeviceStorage {
        id: u32,
        StorageType: u16,
        FilesystemType: u16,
        AccessCapability: u16,
        MaxCapacity: u64,
        FreeSpaceInBytes: u64,
        FreeSpaceInObjects: u64,
        StorageDescription: *mut c_char,
        VolumeIdentifier: *mut c_char,
        next: *mut DeviceStorage,
        prev: *mut DeviceStorage,
    }
    #[repr(C)]
    struct MtpDevice {
        object_bitsize: u8,
        params: *mut c_void,
        usbinfo: *mut c_void,
        storage: *mut DeviceStorage,
        // tail fields omitted (we only ever deref via pointer)
    }

    const FILETYPE_FOLDER: c_int = 0;
    const FILETYPE_UNKNOWN: c_int = 44;
    const PARENT_ROOT: u32 = 0xffff_ffff;

    extern "C" {
        fn LIBMTP_Init();
        fn LIBMTP_Detect_Raw_Devices(devices: *mut *mut RawDevice, n: *mut c_int) -> c_int;
        fn LIBMTP_Open_Raw_Device_Uncached(dev: *mut RawDevice) -> *mut MtpDevice;
        fn LIBMTP_Release_Device(dev: *mut MtpDevice);
        fn LIBMTP_Get_Friendlyname(dev: *mut MtpDevice) -> *mut c_char;
        fn LIBMTP_Get_Modelname(dev: *mut MtpDevice) -> *mut c_char;
        fn LIBMTP_Get_Storage(dev: *mut MtpDevice, sortby: c_int) -> c_int;
        fn LIBMTP_Get_Files_And_Folders(
            dev: *mut MtpDevice,
            storage_id: u32,
            parent_id: u32,
        ) -> *mut MtpFile;
        fn LIBMTP_destroy_file_t(file: *mut MtpFile);
        fn LIBMTP_Get_File_To_File(
            dev: *mut MtpDevice,
            id: u32,
            path: *const c_char,
            cb: Option<ProgressFn>,
            data: *const c_void,
        ) -> c_int;
        fn LIBMTP_Send_File_From_File(
            dev: *mut MtpDevice,
            path: *const c_char,
            filedata: *mut MtpFile,
            cb: Option<ProgressFn>,
            data: *const c_void,
        ) -> c_int;
        fn LIBMTP_Create_Folder(
            dev: *mut MtpDevice,
            name: *mut c_char,
            parent_id: u32,
            storage_id: u32,
        ) -> u32;
        fn LIBMTP_Delete_Object(dev: *mut MtpDevice, id: u32) -> c_int;
        fn LIBMTP_Dump_Errorstack(dev: *mut MtpDevice);
        fn LIBMTP_Clear_Errorstack(dev: *mut MtpDevice);
        fn free(ptr: *mut c_void);
    }

    fn cstr(p: *const c_char) -> String {
        if p.is_null() {
            String::new()
        } else {
            unsafe { CStr::from_ptr(p).to_string_lossy().into_owned() }
        }
    }

    fn take_cstr(p: *mut c_char) -> String {
        if p.is_null() {
            return String::new();
        }
        let s = unsafe { CStr::from_ptr(p).to_string_lossy().into_owned() };
        unsafe { free(p as *mut c_void) };
        s
    }

    pub struct Device {
        handle: *mut MtpDevice,
    }

    impl Device {
        pub fn open_first() -> Result<Device, String> {
            unsafe {
                LIBMTP_Init();
                let mut raw: *mut RawDevice = ptr::null_mut();
                let mut count: c_int = 0;
                let err = LIBMTP_Detect_Raw_Devices(&mut raw, &mut count);
                if err != 0 || count <= 0 || raw.is_null() {
                    if !raw.is_null() {
                        free(raw as *mut c_void);
                    }
                    return Err("no MTP device found (set USB mode to File Transfer)".into());
                }
                let handle = LIBMTP_Open_Raw_Device_Uncached(raw);
                free(raw as *mut c_void);
                if handle.is_null() {
                    return Err("could not open MTP device (in use by another app?)".into());
                }
                Ok(Device { handle })
            }
        }

        pub fn info(&self) -> DeviceInfo {
            let name = {
                let n = take_cstr(unsafe { LIBMTP_Get_Friendlyname(self.handle) });
                if n.is_empty() {
                    let m = take_cstr(unsafe { LIBMTP_Get_Modelname(self.handle) });
                    if m.is_empty() {
                        "Android (MTP)".to_string()
                    } else {
                        m
                    }
                } else {
                    n
                }
            };
            DeviceInfo {
                name,
                storages: self.storages().unwrap_or_default(),
            }
        }

        pub fn storages(&self) -> Result<Vec<Storage>, String> {
            unsafe {
                if LIBMTP_Get_Storage(self.handle, 0) != 0 {
                    return Err("could not read device storage".into());
                }
                let mut out = Vec::new();
                let mut s = (*self.handle).storage;
                while !s.is_null() {
                    let d = cstr((*s).StorageDescription);
                    out.push(Storage {
                        id: (*s).id,
                        description: if d.is_empty() {
                            format!("Storage {}", (*s).id)
                        } else {
                            d
                        },
                        max_capacity: (*s).MaxCapacity,
                        free_bytes: (*s).FreeSpaceInBytes,
                    });
                    s = (*s).next;
                }
                Ok(out)
            }
        }

        pub fn list(&self, storage_id: u32, parent_id: u32) -> Result<Vec<Entry>, String> {
            let parent = if parent_id == 0 { PARENT_ROOT } else { parent_id };
            unsafe {
                let head = LIBMTP_Get_Files_And_Folders(self.handle, storage_id, parent);
                let mut out = Vec::new();
                let mut f = head;
                while !f.is_null() {
                    out.push(Entry {
                        id: (*f).item_id,
                        name: cstr((*f).filename),
                        size: (*f).filesize,
                        mtime: (*f).modificationdate,
                        is_dir: (*f).filetype == FILETYPE_FOLDER,
                    });
                    f = (*f).next;
                }
                let mut f = head;
                while !f.is_null() {
                    let next = (*f).next;
                    LIBMTP_destroy_file_t(f);
                    f = next;
                }
                out.sort_by(|a, b| {
                    b.is_dir
                        .cmp(&a.is_dir)
                        .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                });
                Ok(out)
            }
        }

        pub fn pull(
            &self,
            object_id: u32,
            local: &str,
            progress: Option<ProgressInfo>,
        ) -> Result<(), String> {
            let c = CString::new(local).map_err(|_| "bad local path".to_string())?;
            let ctx = ctx_for(&progress);
            let (cb, data): (Option<ProgressFn>, *const c_void) = match &ctx {
                Some(b) => (Some(progress_cb), &**b as *const ProgressCtx as *const c_void),
                None => (None, ptr::null()),
            };
            let rc = unsafe { LIBMTP_Get_File_To_File(self.handle, object_id, c.as_ptr(), cb, data) };
            if rc == 0 {
                Ok(())
            } else {
                Err("MTP download failed".into())
            }
        }

        pub fn push(
            &self,
            local: &str,
            parent_id: u32,
            storage_id: u32,
            name: &str,
            progress: Option<ProgressInfo>,
        ) -> Result<u32, String> {
            let meta = std::fs::metadata(local).map_err(|e| e.to_string())?;
            let cname = CString::new(name).map_err(|_| "bad name".to_string())?;
            let cpath = CString::new(local).map_err(|_| "bad local path".to_string())?;
            let mut file = MtpFile {
                item_id: 0,
                parent_id: if parent_id == 0 { PARENT_ROOT } else { parent_id },
                storage_id,
                filename: cname.as_ptr() as *mut c_char,
                filesize: meta.len(),
                modificationdate: 0,
                filetype: FILETYPE_UNKNOWN,
                next: ptr::null_mut(),
            };
            let ctx = ctx_for(&progress);
            let (cb, data): (Option<ProgressFn>, *const c_void) = match &ctx {
                Some(b) => (Some(progress_cb), &**b as *const ProgressCtx as *const c_void),
                None => (None, ptr::null()),
            };
            let rc = unsafe {
                LIBMTP_Send_File_From_File(self.handle, cpath.as_ptr(), &mut file, cb, data)
            };
            if rc == 0 {
                Ok(file.item_id)
            } else {
                Err("MTP upload failed".into())
            }
        }

        pub fn mkdir(&self, name: &str, parent_id: u32, storage_id: u32) -> Result<u32, String> {
            let parent = if parent_id == 0 { PARENT_ROOT } else { parent_id };
            let mut buf: Vec<u8> = name.bytes().chain(std::iter::once(0)).collect();
            let id = unsafe {
                LIBMTP_Create_Folder(
                    self.handle,
                    buf.as_mut_ptr() as *mut c_char,
                    parent,
                    storage_id,
                )
            };
            if id == 0 {
                unsafe {
                    LIBMTP_Dump_Errorstack(self.handle);
                    LIBMTP_Clear_Errorstack(self.handle);
                }
                Err("could not create folder".into())
            } else {
                Ok(id)
            }
        }

        pub fn delete(&self, object_id: u32) -> Result<(), String> {
            if unsafe { LIBMTP_Delete_Object(self.handle, object_id) } == 0 {
                Ok(())
            } else {
                Err("MTP delete failed".into())
            }
        }
    }

    impl Drop for Device {
        fn drop(&mut self) {
            unsafe { LIBMTP_Release_Device(self.handle) }
        }
    }
}

// ===================== no feature: stub =====================
#[cfg(not(feature = "mtp"))]
mod imp {
    use super::{DeviceInfo, Entry, ProgressInfo, Storage};

    const MSG: &str = "MTP support is not built into this app (rebuild with --features mtp)";

    pub struct Device;

    impl Device {
        pub fn open_first() -> Result<Device, String> {
            Err(MSG.into())
        }
        pub fn info(&self) -> DeviceInfo {
            DeviceInfo {
                name: String::new(),
                storages: vec![],
            }
        }
        #[allow(dead_code)]
        pub fn storages(&self) -> Result<Vec<Storage>, String> {
            Err(MSG.into())
        }
        pub fn list(&self, _s: u32, _p: u32) -> Result<Vec<Entry>, String> {
            Err(MSG.into())
        }
        pub fn pull(&self, _id: u32, _local: &str, _p: Option<ProgressInfo>) -> Result<(), String> {
            Err(MSG.into())
        }
        pub fn push(
            &self,
            _l: &str,
            _p: u32,
            _s: u32,
            _n: &str,
            _pr: Option<ProgressInfo>,
        ) -> Result<u32, String> {
            Err(MSG.into())
        }
        pub fn mkdir(&self, _n: &str, _p: u32, _s: u32) -> Result<u32, String> {
            Err(MSG.into())
        }
        pub fn delete(&self, _id: u32) -> Result<(), String> {
            Err(MSG.into())
        }
    }
}

use imp::Device;

// ===================== device-owning worker thread =====================

enum Cmd {
    Connect(SyncSender<Result<DeviceInfo, String>>),
    List(u32, u32, SyncSender<Result<Vec<Entry>, String>>),
    Pull(u32, String, Option<ProgressInfo>, SyncSender<Result<(), String>>),
    Push(
        String,
        u32,
        u32,
        String,
        Option<ProgressInfo>,
        SyncSender<Result<u32, String>>,
    ),
    Mkdir(String, u32, u32, SyncSender<Result<u32, String>>),
    Delete(u32, SyncSender<Result<(), String>>),
    Disconnect(SyncSender<Result<(), String>>),
}

fn with<T>(dev: &Option<Device>, f: impl FnOnce(&Device) -> Result<T, String>) -> Result<T, String> {
    match dev {
        Some(d) => f(d),
        None => Err("no MTP device connected".into()),
    }
}

fn worker(rx: Receiver<Cmd>) {
    let mut dev: Option<Device> = None;
    while let Ok(cmd) = rx.recv() {
        match cmd {
            Cmd::Connect(r) => match Device::open_first() {
                Ok(d) => {
                    let info = d.info();
                    dev = Some(d);
                    let _ = r.send(Ok(info));
                }
                Err(e) => {
                    dev = None;
                    let _ = r.send(Err(e));
                }
            },
            Cmd::List(s, p, r) => {
                let _ = r.send(with(&dev, |d| d.list(s, p)));
            }
            Cmd::Pull(id, local, prog, r) => {
                let _ = r.send(with(&dev, |d| d.pull(id, &local, prog)));
            }
            Cmd::Push(local, parent, storage, name, prog, r) => {
                let _ = r.send(with(&dev, |d| d.push(&local, parent, storage, &name, prog)));
            }
            Cmd::Mkdir(name, parent, storage, r) => {
                let _ = r.send(with(&dev, |d| d.mkdir(&name, parent, storage)));
            }
            Cmd::Delete(id, r) => {
                let _ = r.send(with(&dev, |d| d.delete(id)));
            }
            Cmd::Disconnect(r) => {
                dev = None;
                let _ = r.send(Ok(()));
            }
        }
    }
}

/// Handle to the MTP worker thread. Cloneable-free: stored as Tauri state.
pub struct Mtp {
    tx: SyncSender<Cmd>,
}

impl Default for Mtp {
    fn default() -> Self {
        let (tx, rx) = sync_channel::<Cmd>(0);
        std::thread::spawn(move || worker(rx));
        Mtp { tx }
    }
}

impl Mtp {
    fn call<T>(&self, make: impl FnOnce(SyncSender<Result<T, String>>) -> Cmd) -> Result<T, String> {
        let (rtx, rrx) = sync_channel::<Result<T, String>>(0);
        self.tx
            .send(make(rtx))
            .map_err(|_| "MTP worker is not running".to_string())?;
        rrx.recv().map_err(|_| "MTP worker dropped the reply".to_string())?
    }

    pub fn connect(&self) -> Result<DeviceInfo, String> {
        self.call(Cmd::Connect)
    }
    pub fn list(&self, storage: u32, parent: u32) -> Result<Vec<Entry>, String> {
        self.call(|r| Cmd::List(storage, parent, r))
    }
    pub fn pull(&self, id: u32, local: String, prog: Option<ProgressInfo>) -> Result<(), String> {
        self.call(|r| Cmd::Pull(id, local, prog, r))
    }
    pub fn push(
        &self,
        local: String,
        parent: u32,
        storage: u32,
        name: String,
        prog: Option<ProgressInfo>,
    ) -> Result<u32, String> {
        self.call(|r| Cmd::Push(local, parent, storage, name, prog, r))
    }
    pub fn mkdir(&self, name: String, parent: u32, storage: u32) -> Result<u32, String> {
        self.call(|r| Cmd::Mkdir(name, parent, storage, r))
    }
    pub fn delete(&self, id: u32) -> Result<(), String> {
        self.call(|r| Cmd::Delete(id, r))
    }
    pub fn disconnect(&self) -> Result<(), String> {
        self.call(Cmd::Disconnect)
    }
}

/// Probe used by the `mtp_probe` example.
#[cfg(feature = "mtp")]
pub fn probe() -> Result<String, String> {
    let dev = Device::open_first()?;
    let info = dev.info();
    let mut report = format!("Device: {}\nFound {} storage(s):\n", info.name, info.storages.len());
    for s in &info.storages {
        report.push_str(&format!(
            "  [{}] {} — {:.1} GB free of {:.1} GB\n",
            s.id,
            s.description,
            s.free_bytes as f64 / 1e9,
            s.max_capacity as f64 / 1e9,
        ));
    }
    if let Some(s0) = info.storages.first() {
        let entries = dev.list(s0.id, 0)?;
        report.push_str(&format!("\nRoot of \"{}\" ({} entries)\n", s0.description, entries.len()));
        let mut file = entries.iter().find(|e| !e.is_dir).cloned();
        if file.is_none() {
            for dir in entries.iter().filter(|e| e.is_dir).take(6) {
                if let Ok(kids) = dev.list(s0.id, dir.id) {
                    if let Some(f) = kids.into_iter().find(|e| !e.is_dir && e.size > 0) {
                        file = Some(f);
                        break;
                    }
                }
            }
        }
        if let Some(f) = file {
            let dest = std::env::temp_dir().join("freedroid-mtp-test.bin");
            let dest_s = dest.to_string_lossy().to_string();
            report.push_str(&format!("Pull test: \"{}\" ({} bytes)… ", f.name, f.size));
            match dev.pull(f.id, &dest_s, None) {
                Ok(()) => {
                    let got = std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
                    report.push_str(if got == f.size { "SIZE OK\n" } else { "size mismatch\n" });
                    let _ = std::fs::remove_file(&dest);
                }
                Err(e) => report.push_str(&format!("failed: {e}\n")),
            }
        }

        // Write round-trip: mkdir -> push -> verify -> delete file -> delete folder.
        report.push_str("\nWrite test: ");
        let src = std::env::temp_dir().join("freedroid-mtp-up.txt");
        let _ = std::fs::write(&src, b"freedroid mtp write test\n");
        match dev.mkdir("FreedroidMTPTest", 0, s0.id) {
            Ok(folder_id) => {
                report.push_str("mkdir ok, ");
                match dev.push(&src.to_string_lossy(), folder_id, s0.id, "hello.txt", None) {
                    Ok(file_id) => {
                        let listed = dev.list(s0.id, folder_id).unwrap_or_default();
                        let ok = listed.iter().any(|e| e.name == "hello.txt");
                        report.push_str(if ok { "push ok, " } else { "push not visible, " });
                        let _ = dev.delete(file_id);
                        let _ = dev.delete(folder_id);
                        let gone = dev
                            .list(s0.id, 0)
                            .unwrap_or_default()
                            .iter()
                            .all(|e| e.name != "FreedroidMTPTest");
                        report.push_str(if gone { "delete ok\n" } else { "delete left residue\n" });
                    }
                    Err(e) => {
                        let _ = dev.delete(folder_id);
                        report.push_str(&format!("push failed: {e}\n"));
                    }
                }
            }
            Err(e) => report.push_str(&format!("mkdir failed: {e}\n")),
        }
        let _ = std::fs::remove_file(&src);
    }
    Ok(report)
}
