//! Device enumeration via `adb devices -l`.

use crate::error::Result;
use serde::Serialize;
use tauri::AppHandle;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub serial: String,
    /// "device" (ready), "unauthorized" (USB-debugging prompt pending),
    /// "offline", "no permissions", etc.
    pub state: String,
    /// Human-friendly model name when available, else the serial.
    pub model: Option<String>,
    pub product: Option<String>,
    /// True when connected over TCP/IP (Wi-Fi) — serial looks like ip:port.
    pub wifi: bool,
}

impl Device {
    pub fn display_name(&self) -> String {
        self.model
            .clone()
            .map(|m| m.replace('_', " "))
            .unwrap_or_else(|| self.serial.clone())
    }
}

/// Parse the output of `adb devices -l`.
///
/// Format:
/// ```text
/// List of devices attached
/// 0A1B2C3D   device product:foo model:Pixel_7 device:bar transport_id:1
/// 192.168.1.5:5555   device product:...
/// ```
pub fn parse_devices(output: &str) -> Vec<Device> {
    let mut devices = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty()
            || line.starts_with("List of devices")
            || line.starts_with('*')
            || line.starts_with("adb server")
        {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(serial) = parts.next() else {
            continue;
        };
        let Some(state) = parts.next() else {
            continue;
        };

        let mut model = None;
        let mut product = None;
        for tag in parts {
            if let Some(v) = tag.strip_prefix("model:") {
                model = Some(v.to_string());
            } else if let Some(v) = tag.strip_prefix("product:") {
                product = Some(v.to_string());
            }
        }

        let wifi = serial.contains(':') && serial.split(':').next().unwrap_or("").contains('.');

        devices.push(Device {
            serial: serial.to_string(),
            state: state.to_string(),
            model,
            product,
            wifi,
        });
    }
    devices
}

pub async fn list_devices(app: &AppHandle) -> Result<Vec<Device>> {
    let out = super::run(app, &["devices", "-l"]).await?;
    Ok(parse_devices(&out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_usb_and_wifi() {
        let out = "List of devices attached\n\
                   0A1B2C3D\tdevice product:raven model:Pixel_6_Pro device:raven transport_id:1\n\
                   192.168.1.5:5555\tdevice product:x model:Galaxy_S21 device:y transport_id:2\n\
                   FEEDFACE\tunauthorized\n";
        let d = parse_devices(out);
        assert_eq!(d.len(), 3);
        assert_eq!(d[0].serial, "0A1B2C3D");
        assert_eq!(d[0].display_name(), "Pixel 6 Pro");
        assert!(!d[0].wifi);
        assert!(d[1].wifi);
        assert_eq!(d[2].state, "unauthorized");
        assert_eq!(d[2].model, None);
    }
}
