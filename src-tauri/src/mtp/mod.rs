//! Native MTP backend via libmtp (optional `mtp` feature).
//!
//! MTP addresses objects by numeric ids (storage id + object id), not paths, so
//! this layer exposes id-based listing/transfer. A higher layer maps paths to
//! ids for the UI. libmtp device handles are NOT thread-safe, so all access must
//! be serialized on a single owner (see `session`).
//!
//! Struct layouts below mirror `/opt/homebrew/include/libmtp.h` (libmtp 1.1.23)
//! field-for-field; `#[repr(C)]` makes Rust match the C ABI.

#![allow(non_snake_case)]

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_void};
use std::ptr;

// ----- C structs (repr(C) => identical layout to libmtp) -----

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
    modificationdate: i64, // time_t (long) on macOS arm64
    filetype: c_int,       // LIBMTP_filetype_t; FOLDER == 0
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

/// Only the head of LIBMTP_mtpdevice_struct, up to the `storage` pointer we read.
/// We never copy this by value or read past `storage`, so the truncated tail is
/// safe — we only ever hold/deref a pointer libmtp allocated.
#[repr(C)]
struct MtpDevice {
    object_bitsize: u8,
    params: *mut c_void,
    usbinfo: *mut c_void,
    storage: *mut DeviceStorage,
    // ... remaining fields intentionally omitted
}

const FILETYPE_FOLDER: c_int = 0;
const PARENT_ROOT: u32 = 0xffff_ffff;

extern "C" {
    fn LIBMTP_Init();
    fn LIBMTP_Detect_Raw_Devices(devices: *mut *mut RawDevice, n: *mut c_int) -> c_int;
    fn LIBMTP_Open_Raw_Device_Uncached(dev: *mut RawDevice) -> *mut MtpDevice;
    fn LIBMTP_Release_Device(dev: *mut MtpDevice);
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
        cb: *const c_void,
        data: *const c_void,
    ) -> c_int;
    fn free(ptr: *mut c_void);
}

// ----- Safe-ish Rust API -----

#[derive(Debug, Clone)]
pub struct Storage {
    pub id: u32,
    pub description: String,
    pub max_capacity: u64,
    pub free_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub id: u32,
    pub name: String,
    pub size: u64,
    pub mtime: i64,
    pub is_dir: bool,
}

fn cstr(p: *const c_char) -> String {
    if p.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(p).to_string_lossy().into_owned() }
}

/// An opened MTP device. Not `Send`/`Sync` — the raw pointer must stay on one
/// thread. Drop releases the device.
pub struct Device {
    handle: *mut MtpDevice,
}

impl Device {
    /// Detect and open the first connected MTP device.
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
                return Err("no MTP devices found".into());
            }
            // Open the first device; free the detected array afterwards.
            let handle = LIBMTP_Open_Raw_Device_Uncached(raw);
            free(raw as *mut c_void);
            if handle.is_null() {
                return Err("failed to open MTP device (is it in File Transfer mode and not claimed by another app?)".into());
            }
            Ok(Device { handle })
        }
    }

    /// List storages (internal storage, SD card, …).
    pub fn storages(&self) -> Result<Vec<Storage>, String> {
        unsafe {
            if LIBMTP_Get_Storage(self.handle, 0) != 0 {
                return Err("could not read device storage".into());
            }
            let mut out = Vec::new();
            let mut s = (*self.handle).storage;
            while !s.is_null() {
                out.push(Storage {
                    id: (*s).id,
                    description: {
                        let d = cstr((*s).StorageDescription);
                        if d.is_empty() {
                            format!("Storage {}", (*s).id)
                        } else {
                            d
                        }
                    },
                    max_capacity: (*s).MaxCapacity,
                    free_bytes: (*s).FreeSpaceInBytes,
                });
                s = (*s).next;
            }
            Ok(out)
        }
    }

    /// List a folder's children. Use `PARENT_ROOT`-equivalent (`parent_id == 0`
    /// maps to the storage root).
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
            // Free the returned linked list.
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

    /// Download an object to a local path.
    pub fn pull(&self, object_id: u32, local: &str) -> Result<(), String> {
        let c = CString::new(local).map_err(|_| "bad local path".to_string())?;
        let rc = unsafe {
            LIBMTP_Get_File_To_File(
                self.handle,
                object_id,
                c.as_ptr(),
                ptr::null(),
                ptr::null(),
            )
        };
        if rc == 0 {
            Ok(())
        } else {
            Err("MTP download failed".into())
        }
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        unsafe { LIBMTP_Release_Device(self.handle) }
    }
}

/// Probe used by the `mtp_probe` example: open the first device and dump its
/// storages and the root listing of the first storage.
pub fn probe() -> Result<String, String> {
    let dev = Device::open_first()?;
    let mut report = String::new();
    let storages = dev.storages()?;
    report.push_str(&format!("Found {} storage(s):\n", storages.len()));
    for s in &storages {
        report.push_str(&format!(
            "  [{}] {} — {:.1} GB free of {:.1} GB\n",
            s.id,
            s.description,
            s.free_bytes as f64 / 1e9,
            s.max_capacity as f64 / 1e9,
        ));
    }
    if let Some(s0) = storages.first() {
        let entries = dev.list(s0.id, 0)?;
        report.push_str(&format!(
            "\nRoot of \"{}\" ({} entries):\n",
            s0.description,
            entries.len()
        ));
        for e in entries.iter().take(40) {
            report.push_str(&format!(
                "  {} {:>12}  {}\n",
                if e.is_dir { "[DIR ]" } else { "[file]" },
                e.size,
                e.name
            ));
        }

        // Find one real file (descend a couple of folders) and test a pull.
        let mut file: Option<Entry> = entries.iter().find(|e| !e.is_dir).cloned();
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
        match file {
            Some(f) => {
                let dest = std::env::temp_dir().join("freedroid-mtp-test.bin");
                let dest_s = dest.to_string_lossy().to_string();
                report.push_str(&format!("\nPull test: \"{}\" ({} bytes)…\n", f.name, f.size));
                match dev.pull(f.id, &dest_s) {
                    Ok(()) => {
                        let got = std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
                        report.push_str(&format!(
                            "  downloaded {got} bytes -> {dest_s} ({})\n",
                            if got == f.size { "SIZE OK" } else { "size mismatch" }
                        ));
                        let _ = std::fs::remove_file(&dest);
                    }
                    Err(e) => report.push_str(&format!("  pull failed: {e}\n")),
                }
            }
            None => report.push_str("\n(no files found to test a pull)\n"),
        }
    }
    Ok(report)
}
