use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::Instant;

fn main() {
    let iterations = 20_000_u64;
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind tcp listener");
    let addr = listener.local_addr().expect("listener addr");

    let server = thread::spawn(move || {
        let (mut socket, _) = listener.accept().expect("accept");
        let mut buf = [0_u8; 8];
        for _ in 0..iterations {
            // Echo payload back to model request/response roundtrip over loopback TCP.
            socket.read_exact(&mut buf).expect("server read");
            socket.write_all(&buf).expect("server write");
        }
    });

    let mut client = TcpStream::connect(addr).expect("connect");
    client.set_nodelay(true).expect("nodelay");
    let start = Instant::now();
    for i in 0..iterations {
        let payload = i.to_le_bytes();
        client.write_all(&payload).expect("client write");
        let mut response = [0_u8; 8];
        client.read_exact(&mut response).expect("client read");
        assert_eq!(u64::from_le_bytes(response), i);
    }
    let elapsed = start.elapsed();
    let per_roundtrip_ns = elapsed.as_nanos() / u128::from(iterations);
    server.join().expect("server join");

    println!("tcp-loop: {iterations} roundtrips in {elapsed:?}, {per_roundtrip_ns} ns/rt");
}
