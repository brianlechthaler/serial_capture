use super::*;
use std::fs::File;
use std::io::Write;

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let mut file = File::create(path).unwrap();
    file.write_all(contents.as_bytes()).unwrap();
}

#[test]
fn usb_tty_names() {
    assert!(is_usb_tty_name("ttyUSB0"));
    assert!(is_usb_tty_name("/dev/ttyACM1"));
    assert!(is_usb_tty_name("cu.usbserial-123"));
    assert!(is_usb_tty_name("tty.usbmodem14101"));
    assert!(is_usb_tty_name("cu.wchusbserial110"));
    assert!(is_usb_tty_name("tty.wchusbserial110"));
    assert!(is_usb_tty_name("COM3"));
    assert!(is_usb_tty_name("com12"));
    assert!(is_usb_tty_name(r"C:\COM1"));
    assert!(!is_usb_tty_name("ttyS0"));
    assert!(!is_usb_tty_name("COM"));
    assert!(!is_usb_tty_name("COMM1"));
    assert!(!is_usb_tty_name("random"));
}

#[test]
fn identity_variants() {
    let path_only = Device::from_path("/dev/ttyUSB0");
    assert_eq!(path_only.id(), "/dev/ttyUSB0");
    assert_eq!(path_only.display_line(), "/dev/ttyUSB0");

    let with_serial = Device {
        path: "/dev/ttyUSB0".into(),
        usb: Some(UsbInfo {
            vid: 0x2341,
            pid: 0x0043,
            serial: Some("ABC".into()),
            manufacturer: Some("Arduino".into()),
            product: Some("Uno".into()),
        }),
    };
    assert_eq!(with_serial.id(), "2341:0043:ABC");
    assert_eq!(
        with_serial.display_line(),
        "/dev/ttyUSB0  2341:0043  serial=ABC  Uno"
    );

    let no_serial = Device {
        path: "/dev/ttyUSB1".into(),
        usb: Some(UsbInfo {
            vid: 0x2341,
            pid: 0x0043,
            serial: None,
            manufacturer: Some("Arduino".into()),
            product: None,
        }),
    };
    assert_eq!(no_serial.id(), "2341:0043:/dev/ttyUSB1");
    assert_eq!(no_serial.display_line(), "/dev/ttyUSB1  2341:0043  Arduino");

    let empty_serial = Device {
        path: "/dev/ttyUSB2".into(),
        usb: Some(UsbInfo {
            vid: 1,
            pid: 2,
            serial: Some(String::new()),
            manufacturer: None,
            product: None,
        }),
    };
    assert_eq!(empty_serial.id(), "0001:0002:/dev/ttyUSB2");
    assert_eq!(empty_serial.display_line(), "/dev/ttyUSB2  0001:0002");
}

#[test]
fn scan_missing_dir_is_empty() {
    assert!(scan(Path::new("/no/such/dev/dir"), Path::new("/no/sys")).is_empty());
}

#[cfg(unix)]
#[test]
fn scan_skips_non_utf8_names() {
    use std::os::unix::ffi::OsStringExt;
    let root = std::env::temp_dir().join(format!("serial-capture-nonutf8-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let dev = root.join("dev");
    fs::create_dir_all(&dev).unwrap();
    let mut raw = std::ffi::OsString::from(dev.as_os_str());
    raw.push("/");
    raw.push(std::ffi::OsString::from_vec(vec![0xff, 0xfe]));
    File::create(&raw).unwrap();
    let devices = scan(&dev, Path::new("/no/sys"));
    fs::remove_dir_all(&root).ok();
    assert!(devices.is_empty());
}

#[test]
fn scan_finds_usb_and_sysfs() {
    let root = std::env::temp_dir().join(format!("serial-capture-scan-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let dev = root.join("dev");
    let sys = root.join("sys/class/tty");
    write(&dev.join("ttyUSB0"), "");
    write(&dev.join("ttyS0"), "");
    write(&dev.join(".hidden"), "");
    write(&sys.join("ttyUSB0/device/.keep"), "");
    write(&sys.join("ttyUSB0/idVendor"), "2341\n");
    write(&sys.join("ttyUSB0/idProduct"), "0043\n");
    write(&sys.join("ttyUSB0/serial"), "SN1\n");
    write(&sys.join("ttyUSB0/manufacturer"), "Arduino\n");
    write(&sys.join("ttyUSB0/product"), "Uno\n");
    write(&dev.join("ttyACM0"), "");

    let devices = scan(&dev, &sys);
    fs::remove_dir_all(&root).ok();
    assert_eq!(devices.len(), 2);
    assert_eq!(devices[0].path, dev.join("ttyACM0").display().to_string());
    assert!(devices[0].usb.is_none());
    assert_eq!(devices[1].path, dev.join("ttyUSB0").display().to_string());
    let usb = devices[1].usb.as_ref().unwrap();
    assert_eq!(usb.vid, 0x2341);
    assert_eq!(usb.pid, 0x0043);
    assert_eq!(usb.serial.as_deref(), Some("SN1"));
    assert_eq!(usb.product.as_deref(), Some("Uno"));
}

#[test]
fn usb_info_walks_parents_and_rejects_bad_hex() {
    let root = std::env::temp_dir().join(format!("serial-capture-sys-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let sys = root.join("sys/class/tty");
    write(&sys.join("ttyUSB0/device/dummy"), "");
    write(&sys.join("ttyUSB0/idVendor"), "zz\n");
    write(&sys.join("ttyUSB0/idProduct"), "0001\n");
    assert!(usb_info(&sys, "ttyUSB0").is_none());

    write(&sys.join("ttyUSB1/device/nested/x"), "");
    write(&sys.join("ttyUSB1/idVendor"), "0001\n");
    write(&sys.join("ttyUSB1/idProduct"), "0002\n");
    write(&sys.join("ttyUSB1/serial"), "   \n");
    let info = usb_info(&sys, "ttyUSB1").unwrap();
    assert_eq!(info.vid, 1);
    assert_eq!(info.pid, 2);
    assert!(info.serial.is_none());

    write(&sys.join("ttyUSB2/device/a/b/c/d/e/f/g/h/i/j/k/x"), "");
    assert!(usb_info(&sys, "ttyUSB2").is_none());
    fs::remove_dir_all(&root).ok();
}

#[test]
fn merge_fills_usb_and_skips_non_usb() {
    let a = vec![Device::from_path("/dev/ttyUSB0")];
    let b = vec![
        Device {
            path: "/dev/ttyUSB0".into(),
            usb: Some(UsbInfo {
                vid: 1,
                pid: 2,
                serial: None,
                manufacturer: None,
                product: None,
            }),
        },
        Device::from_path("/dev/ttyUSB1"),
        Device::from_path("/dev/ttyS0"),
    ];
    let merged = merge(a, b);
    assert_eq!(merged.len(), 2);
    assert!(merged[0].usb.is_some());
    assert_eq!(merged[1].path, "/dev/ttyUSB1");
}

#[test]
fn merge_keeps_existing_usb() {
    let a = vec![Device {
        path: "/dev/ttyUSB0".into(),
        usb: Some(UsbInfo {
            vid: 1,
            pid: 1,
            serial: Some("keep".into()),
            manufacturer: None,
            product: None,
        }),
    }];
    let b = vec![Device {
        path: "/dev/ttyUSB0".into(),
        usb: Some(UsbInfo {
            vid: 9,
            pid: 9,
            serial: Some("drop".into()),
            manufacturer: None,
            product: None,
        }),
    }];
    let merged = merge(a, b);
    assert_eq!(
        merged[0].usb.as_ref().unwrap().serial.as_deref(),
        Some("keep")
    );
}

#[test]
fn selector_from_devices() {
    assert_eq!(Selector::from_devices(&[]), Selector::Auto);
    assert_eq!(
        Selector::from_devices(&["/dev/ttyUSB0".into()]),
        Selector::Paths(vec!["/dev/ttyUSB0".into()])
    );
}

#[test]
fn auto_targets_note_identity() {
    let mut registry = Registry::default();
    let discovered = vec![Device {
        path: "/dev/ttyUSB0".into(),
        usb: Some(UsbInfo {
            vid: 1,
            pid: 2,
            serial: Some("SN".into()),
            manufacturer: None,
            product: None,
        }),
    }];
    let found = targets(&Selector::Auto, &discovered, &mut registry);
    assert_eq!(found[0].key, "0001:0002:SN");
    assert_eq!(registry.resolve("0001:0002:SN"), "/dev/ttyUSB0");
}

#[test]
fn specified_path_waits_then_remaps_by_serial() {
    let mut registry = Registry::default();
    let selector = Selector::Paths(vec!["/dev/ttyUSB0".into()]);
    let waiting = targets(&selector, &[], &mut registry);
    assert_eq!(waiting[0].path, "/dev/ttyUSB0");

    let first = Device {
        path: "/dev/ttyUSB0".into(),
        usb: Some(UsbInfo {
            vid: 1,
            pid: 2,
            serial: Some("SN".into()),
            manufacturer: None,
            product: None,
        }),
    };
    let bound = targets(&selector, &[first], &mut registry);
    assert_eq!(bound[0].path, "/dev/ttyUSB0");

    let moved = Device {
        path: "/dev/ttyUSB1".into(),
        usb: Some(UsbInfo {
            vid: 1,
            pid: 2,
            serial: Some("SN".into()),
            manufacturer: None,
            product: None,
        }),
    };
    let remapped = targets(&selector, &[moved], &mut registry);
    assert_eq!(remapped[0].path, "/dev/ttyUSB1");
    assert_eq!(registry.resolve("/dev/ttyUSB0"), "/dev/ttyUSB1");
}

#[test]
fn write_devices_lists_lines() {
    let mut buf = Vec::new();
    write_devices(&[Device::from_path("/dev/ttyUSB0")], &mut buf).unwrap();
    assert_eq!(String::from_utf8(buf).unwrap(), "/dev/ttyUSB0\n");
}

#[test]
fn list_devices_does_not_panic() {
    let _ = list_devices();
    let _ = from_serialport();
}

#[test]
fn resolve_unknown_key_is_passthrough() {
    let registry = Registry::default();
    assert_eq!(registry.resolve("/dev/ttyUSB9"), "/dev/ttyUSB9");
}

#[test]
fn device_from_port_variants() {
    use serialport::{SerialPortInfo, SerialPortType, UsbPortInfo};
    let usb = device_from_port(SerialPortInfo {
        port_name: "/dev/ttyUSB0".into(),
        port_type: SerialPortType::UsbPort(UsbPortInfo {
            vid: 1,
            pid: 2,
            serial_number: Some("S".into()),
            manufacturer: Some("M".into()),
            product: Some("P".into()),
        }),
    })
    .unwrap();
    assert_eq!(usb.usb.as_ref().unwrap().serial.as_deref(), Some("S"));
    assert_eq!(usb.usb.as_ref().unwrap().manufacturer.as_deref(), Some("M"));
    assert_eq!(usb.usb.as_ref().unwrap().product.as_deref(), Some("P"));

    let empty = device_from_port(SerialPortInfo {
        port_name: "COM1".into(),
        port_type: SerialPortType::UsbPort(UsbPortInfo {
            vid: 1,
            pid: 2,
            serial_number: Some(String::new()),
            manufacturer: Some(String::new()),
            product: Some(String::new()),
        }),
    })
    .unwrap();
    assert!(empty.usb.as_ref().unwrap().serial.is_none());

    assert!(device_from_port(SerialPortInfo {
        port_name: "/dev/ttyS0".into(),
        port_type: SerialPortType::PciPort,
    })
    .is_none());
    assert!(device_from_port(SerialPortInfo {
        port_name: "/dev/ttyUSB1".into(),
        port_type: SerialPortType::PciPort,
    })
    .unwrap()
    .usb
    .is_none());
    assert!(device_from_port(SerialPortInfo {
        port_name: "/dev/ttyUSB2".into(),
        port_type: SerialPortType::BluetoothPort,
    })
    .unwrap()
    .usb
    .is_none());
    assert!(device_from_port(SerialPortInfo {
        port_name: "/dev/ttyUSB3".into(),
        port_type: SerialPortType::Unknown,
    })
    .unwrap()
    .usb
    .is_none());
}
