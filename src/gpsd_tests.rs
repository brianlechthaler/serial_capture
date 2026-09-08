use super::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[test]
fn parse_tpv_reads_lat_lon() {
    let pos = parse_tpv(r#"{"class":"TPV","mode":3,"lat":37.5,"lon":-122.25}"#).unwrap();
    assert_eq!(pos.lat, 37.5);
    assert_eq!(pos.lon, -122.25);
}

#[test]
fn parse_tpv_accepts_integer_coords() {
    let pos = parse_tpv(r#"{"class":"TPV","lat":37,"lon":-122}"#).unwrap();
    assert_eq!(pos.lat, 37.0);
    assert_eq!(pos.lon, -122.0);
}

#[test]
fn parse_tpv_ignores_non_fix() {
    assert!(parse_tpv(r#"{"class":"TPV","mode":1}"#).is_none());
    assert!(parse_tpv(r#"{"class":"SKY"}"#).is_none());
    assert!(parse_tpv(r#"{"class":"VERSION","release":"3.25"}"#).is_none());
    assert!(parse_tpv("not json").is_none());
    assert!(parse_tpv(r#"{"class":"TPV","lat":1}"#).is_none());
}

#[test]
fn connect_rejects_bad_address() {
    let err = connect("256.256.256.256:2947", Duration::from_millis(50)).unwrap_err();
    assert!(!err.to_string().is_empty());
    let err = connect("nope", Duration::from_millis(50)).unwrap_err();
    assert!(!err.to_string().is_empty());
}

#[test]
fn watch_stores_tpv_and_stops() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut sock, _) = listener.accept().unwrap();
        let mut buf = [0u8; 128];
        let _ = sock.read(&mut buf);
        sock.write_all(b"{\"class\":\"VERSION\"}\n{\"class\":\"TPV\",\"lat\":1.5,\"lon\":2.25}\n")
            .unwrap();
        thread::sleep(Duration::from_millis(400));
    });
    let latest = Mutex::new(None);
    let stop = AtomicBool::new(false);
    let addr = addr.to_string();
    thread::scope(|scope| {
        scope.spawn(|| {
            watch(
                &addr,
                &latest,
                &stop,
                thread::sleep,
                Duration::from_millis(50),
            );
        });
        let start = Instant::now();
        while latest.lock().unwrap().is_none() {
            assert!(start.elapsed() < Duration::from_secs(3), "gpsd TPV timeout");
            thread::sleep(Duration::from_millis(5));
        }
        stop.store(true, Ordering::Relaxed);
    });
    assert_eq!(
        *latest.lock().unwrap(),
        Some(GpsPosition {
            lat: 1.5,
            lon: 2.25
        })
    );
    let _ = server.join();
}

#[test]
fn watch_retries_refused_then_stops() {
    let stop = AtomicBool::new(false);
    let latest = Mutex::new(None);
    let sleeps = Arc::new(Mutex::new(0usize));
    let sleep_count = sleeps.clone();
    thread::scope(|scope| {
        scope.spawn(|| {
            watch(
                "127.0.0.1:1",
                &latest,
                &stop,
                {
                    let sleeps = sleep_count.clone();
                    move |_| {
                        *sleeps.lock().unwrap() += 1;
                    }
                },
                Duration::from_millis(1),
            );
        });
        let start = Instant::now();
        while *sleeps.lock().unwrap() < 2 {
            assert!(start.elapsed() < Duration::from_secs(3), "retry timeout");
            thread::sleep(Duration::from_millis(5));
        }
        stop.store(true, Ordering::Relaxed);
    });
    assert!(latest.lock().unwrap().is_none());
}

#[test]
fn watch_reconnects_after_eof() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let connects = Arc::new(Mutex::new(0usize));
    let server_connects = connects.clone();
    let server = thread::spawn(move || {
        for _ in 0..2 {
            let (mut sock, _) = listener.accept().unwrap();
            *server_connects.lock().unwrap() += 1;
            let mut buf = [0u8; 128];
            let _ = sock.read(&mut buf);
        }
    });
    let latest = Mutex::new(None);
    let stop = AtomicBool::new(false);
    let addr = addr.to_string();
    thread::scope(|scope| {
        scope.spawn(|| {
            watch(
                &addr,
                &latest,
                &stop,
                thread::sleep,
                Duration::from_millis(10),
            );
        });
        let start = Instant::now();
        while *connects.lock().unwrap() < 2 {
            assert!(
                start.elapsed() < Duration::from_secs(3),
                "eof reconnect timeout"
            );
            thread::sleep(Duration::from_millis(5));
        }
        stop.store(true, Ordering::Relaxed);
    });
    let _ = server.join();
}

#[test]
fn lock_recovers_poisoned_mutex() {
    let mutex = Mutex::new(Some(GpsPosition { lat: 1.0, lon: 2.0 }));
    let poisoned = Arc::new(mutex);
    let clone = poisoned.clone();
    let _ = thread::spawn(move || {
        let _guard = clone.lock().unwrap();
        panic!("poison");
    })
    .join();
    assert_eq!(lock(poisoned.as_ref()).unwrap().lat, 1.0);
}

#[test]
fn watch_times_out_until_stop() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut sock, _) = listener.accept().unwrap();
        let mut buf = [0u8; 128];
        let _ = sock.read(&mut buf);
        thread::sleep(Duration::from_millis(350));
    });
    let latest = Mutex::new(None);
    let stop = AtomicBool::new(false);
    let addr = addr.to_string();
    thread::scope(|scope| {
        scope.spawn(|| {
            watch(
                &addr,
                &latest,
                &stop,
                thread::sleep,
                Duration::from_millis(50),
            );
        });
        thread::sleep(Duration::from_millis(150));
        stop.store(true, Ordering::Relaxed);
    });
    assert!(latest.lock().unwrap().is_none());
    let _ = server.join();
}

#[test]
fn connect_fails_unroutable() {
    assert!(connect("192.0.2.1:2947", Duration::from_millis(50)).is_err());
}

#[test]
fn read_fixes_returns_on_write_error() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (sock, _) = listener.accept().unwrap();
        drop(sock);
    });
    let stream = connect(&addr.to_string(), Duration::from_secs(1)).unwrap();
    server.join().unwrap();
    let latest = Mutex::new(None);
    let stop = AtomicBool::new(false);
    read_fixes(stream, &latest, &stop);
    assert!(latest.lock().unwrap().is_none());
}
