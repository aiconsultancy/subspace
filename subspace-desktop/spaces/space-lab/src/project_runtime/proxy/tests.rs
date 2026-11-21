use super::{DestinationPolicy, PersistentProxy};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::{Duration, Instant};

fn open(proxy: &PersistentProxy) -> TcpStream {
    let address = proxy.address().trim_start_matches("http://").to_owned();
    let client = TcpStream::connect(address).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    client
}

fn connect(proxy: &PersistentProxy, destination: SocketAddr) -> TcpStream {
    let mut client = open(proxy);
    write!(
        client,
        "CONNECT {destination} HTTP/1.1\r\nHost: {destination}\r\n\r\n"
    )
    .unwrap();
    client
}

fn response(client: &TcpStream) -> String {
    let mut reader = BufReader::new(client);
    let mut header = String::new();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        if line.is_empty() || line == "\r\n" {
            break;
        }
        header.push_str(&line);
    }
    header
}

#[test]
fn project_runtime_normal_policy_uses_project_https_origins_and_denies_private_or_non_https() {
    assert!(DestinationPolicy::new(&["https://127.0.0.1".into()]).is_err());
    assert!(DestinationPolicy::new(&["https://localhost".into()]).is_err());
    assert!(DestinationPolicy::new(&["https://10.0.0.1".into()]).is_err());
    assert!(DestinationPolicy::new(&["https://[::1]".into()]).is_err());
    assert!(DestinationPolicy::new(&["http://example.invalid".into()]).is_err());
    // Numeric public fixture only validates policy. It makes no connection.
    assert!(DestinationPolicy::new(&["https://8.8.8.8:8443".into()]).is_ok());
    assert!(DestinationPolicy::new(&["https://8.8.8.8/path".into()]).is_err());
}

#[test]
fn project_runtime_proxy_denies_ungranted_destination_without_contact() {
    let fixture = TcpListener::bind("127.0.0.1:0").unwrap();
    fixture.set_nonblocking(true).unwrap();
    let proxy = PersistentProxy::start(DestinationPolicy::default()).unwrap();
    let client = connect(&proxy, fixture.local_addr().unwrap());
    assert!(response(&client).starts_with("HTTP/1.1 403"));
    assert_eq!(
        fixture.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[test]
fn project_runtime_persistent_proxy_survives_more_than_32_requests_and_reaps_on_drop() {
    let fixture = TcpListener::bind("127.0.0.1:0").unwrap();
    let destination = fixture.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        for _ in 0..40 {
            let (mut stream, _) = fixture.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut byte = [0u8; 1];
            stream.read_exact(&mut byte).unwrap();
            stream.write_all(&byte).unwrap();
        }
    });
    let proxy = PersistentProxy::start(DestinationPolicy::local_fixture(destination)).unwrap();
    for ix in 0u8..40 {
        let mut client = connect(&proxy, destination);
        assert!(response(&client).starts_with("HTTP/1.1 200"));
        client.write_all(&[ix]).unwrap();
        let mut byte = [0u8; 1];
        client.read_exact(&mut byte).unwrap();
        assert_eq!(byte, [ix]);
        // Let a completed worker release its slot without introducing a TTL.
        drop(client);
    }
    server.join().unwrap();
    let before = Instant::now();
    drop(proxy);
    assert!(before.elapsed() < Duration::from_secs(2));
}

#[test]
fn project_runtime_proxy_has_eight_active_slots_and_stop_cancels_idle_tunnels() {
    let fixture = TcpListener::bind("127.0.0.1:0").unwrap();
    let destination = fixture.local_addr().unwrap();
    let proxy = PersistentProxy::start(DestinationPolicy::local_fixture(destination)).unwrap();
    let mut clients = Vec::new();
    let mut upstreams = Vec::new();
    for _ in 0..8 {
        let client = connect(&proxy, destination);
        assert!(response(&client).starts_with("HTTP/1.1 200"));
        clients.push(client);
        upstreams.push(fixture.accept().unwrap().0);
    }
    // At capacity the proxy answers when it accepts and closes without
    // reading, so a request written here would race that close.
    let overflow = open(&proxy);
    assert!(response(&overflow).starts_with("HTTP/1.1 503"));
    let before = Instant::now();
    drop(proxy);
    assert!(before.elapsed() < Duration::from_secs(2));
    for mut client in clients {
        let mut byte = [0u8; 1];
        assert!(matches!(client.read(&mut byte), Ok(0) | Err(_)));
    }
}
