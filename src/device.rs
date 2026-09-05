use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsbInfo {
    pub vid: u16,
    pub pid: u16,
    pub serial: Option<String>,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub path: String,
    pub usb: Option<UsbInfo>,
}

impl Device {
    pub fn from_path(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            usb: None,
        }
    }

    pub fn id(&self) -> String {
        match &self.usb {
            Some(usb) => match &usb.serial {
                Some(serial) if !serial.is_empty() => {
                    format!("{:04x}:{:04x}:{serial}", usb.vid, usb.pid)
                }
                _ => format!("{:04x}:{:04x}:{}", usb.vid, usb.pid, self.path),
            },
            None => self.path.clone(),
        }
    }

    pub fn display_line(&self) -> String {
        match &self.usb {
            Some(usb) => {
                let mut line = format!("{}  {:04x}:{:04x}", self.path, usb.vid, usb.pid);
                if let Some(serial) = usb.serial.as_deref().filter(|s| !s.is_empty()) {
                    line.push_str("  serial=");
                    line.push_str(serial);
                }
                if let Some(product) = usb.product.as_deref().filter(|s| !s.is_empty()) {
                    line.push_str("  ");
                    line.push_str(product);
                } else if let Some(manufacturer) =
                    usb.manufacturer.as_deref().filter(|s| !s.is_empty())
                {
                    line.push_str("  ");
                    line.push_str(manufacturer);
                }
                line
            }
            None => self.path.clone(),
        }
    }
}

pub fn is_usb_tty_name(name: &str) -> bool {
    let name = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let lower = name.to_ascii_lowercase();
    if lower.starts_with("ttyusb")
        || lower.starts_with("ttyacm")
        || lower.starts_with("cu.usb")
        || lower.starts_with("tty.usb")
        || lower.starts_with("cu.wchusb")
        || lower.starts_with("tty.wchusb")
    {
        return true;
    }
    let digits = if let Some(rest) = name.strip_prefix("COM") {
        rest
    } else if let Some(rest) = name.strip_prefix("com") {
        rest
    } else {
        return false;
    };
    !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())
}

pub fn scan(dev_dir: &Path, sys_class_tty: &Path) -> Vec<Device> {
    let Ok(entries) = fs::read_dir(dev_dir) else {
        return Vec::new();
    };
    let mut devices = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if name.starts_with('.') || !is_usb_tty_name(name) {
            continue;
        }
        devices.push(Device {
            path: entry.path().display().to_string(),
            usb: usb_info(sys_class_tty, name),
        });
    }
    devices.sort_by(|a, b| a.path.cmp(&b.path));
    devices
}

pub fn usb_info(sys_class_tty: &Path, tty_name: &str) -> Option<UsbInfo> {
    let mut dir = sys_class_tty.join(tty_name).join("device");
    loop {
        let vid_path = dir.join("idVendor");
        if vid_path.is_file() {
            let vid = parse_hex(&fs::read_to_string(vid_path).ok()?)?;
            let pid = parse_hex(&fs::read_to_string(dir.join("idProduct")).ok()?)?;
            return Some(UsbInfo {
                vid,
                pid,
                serial: read_trimmed(dir.join("serial")),
                manufacturer: read_trimmed(dir.join("manufacturer")),
                product: read_trimmed(dir.join("product")),
            });
        }
        dir = dir.parent()?.to_path_buf();
    }
}

fn read_trimmed(path: PathBuf) -> Option<String> {
    let value = fs::read_to_string(path).ok()?;
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn parse_hex(s: &str) -> Option<u16> {
    u16::from_str_radix(s.trim(), 16).ok()
}

pub fn device_from_port(port: serialport::SerialPortInfo) -> Option<Device> {
    if !is_usb_tty_name(&port.port_name) {
        return None;
    }
    let usb = match port.port_type {
        serialport::SerialPortType::UsbPort(info) => Some(UsbInfo {
            vid: info.vid,
            pid: info.pid,
            serial: info.serial_number.filter(|s| !s.is_empty()),
            manufacturer: info.manufacturer.filter(|s| !s.is_empty()),
            product: info.product.filter(|s| !s.is_empty()),
        }),
        serialport::SerialPortType::PciPort
        | serialport::SerialPortType::BluetoothPort
        | serialport::SerialPortType::Unknown => None,
    };
    Some(Device {
        path: port.port_name,
        usb,
    })
}

pub fn from_serialport() -> Vec<Device> {
    serialport::available_ports()
        .unwrap_or_default()
        .into_iter()
        .filter_map(device_from_port)
        .collect()
}

pub fn merge(mut primary: Vec<Device>, extra: Vec<Device>) -> Vec<Device> {
    for device in extra {
        if let Some(existing) = primary.iter_mut().find(|d| d.path == device.path) {
            if existing.usb.is_none() {
                existing.usb = device.usb;
            }
        } else if is_usb_tty_name(&device.path) {
            primary.push(device);
        }
    }
    primary.sort_by(|a, b| a.path.cmp(&b.path));
    primary
}

pub fn list_devices() -> Vec<Device> {
    merge(
        scan(Path::new("/dev"), Path::new("/sys/class/tty")),
        from_serialport(),
    )
}

pub fn write_devices(devices: &[Device], writer: &mut impl std::io::Write) -> std::io::Result<()> {
    for device in devices {
        writeln!(writer, "{}", device.display_line())?;
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selector {
    Auto,
    Paths(Vec<String>),
}

impl Selector {
    pub fn from_devices(devices: &[String]) -> Self {
        if devices.is_empty() {
            Self::Auto
        } else {
            Self::Paths(devices.to_vec())
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct Registry {
    id_to_path: HashMap<String, String>,
    key_to_id: HashMap<String, String>,
}

impl Registry {
    pub fn note(&mut self, device: &Device) {
        let id = device.id();
        self.id_to_path.insert(id.clone(), device.path.clone());
        self.key_to_id.insert(device.path.clone(), id);
    }

    pub fn bind(&mut self, requested: String, device: &Device) {
        self.note(device);
        self.key_to_id.insert(requested, device.id());
    }

    pub fn resolve(&self, key: &str) -> String {
        if let Some(path) = self.id_to_path.get(key) {
            return path.clone();
        }
        self.key_to_id
            .get(key)
            .and_then(|id| self.id_to_path.get(id))
            .cloned()
            .unwrap_or_else(|| key.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub key: String,
    pub path: String,
}

pub fn targets(selector: &Selector, discovered: &[Device], registry: &mut Registry) -> Vec<Target> {
    match selector {
        Selector::Auto => discovered
            .iter()
            .map(|device| {
                registry.note(device);
                Target {
                    key: device.id(),
                    path: device.path.clone(),
                }
            })
            .collect(),
        Selector::Paths(paths) => {
            for device in discovered {
                for requested in paths {
                    if requested == &device.path
                        || registry.key_to_id.get(requested).map(String::as_str)
                            == Some(device.id()).as_deref()
                    {
                        registry.bind(requested.clone(), device);
                    }
                }
            }
            paths
                .iter()
                .map(|path| Target {
                    key: path.clone(),
                    path: registry.resolve(path),
                })
                .collect()
        }
    }
}

#[cfg(test)]
#[path = "device_tests.rs"]
mod device_tests;
