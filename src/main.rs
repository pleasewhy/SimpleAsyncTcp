#![feature(allocator_api)]
#![feature(get_mut_unchecked)]

use crate::net::{tcplisten, tcpstream};
use std::net::SocketAddr;
use std::net::{IpAddr, Ipv4Addr};

#[macro_use]
mod net;
mod executor;
mod timer_future;
mod waker_page;


fn main() {
    executor::spawn(server_main());
    crate::net::init().unwrap();
    executor::run();
}

async fn server_main() {
    let mut listener = tcplisten::bind_addr(SocketAddr::new(
        IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
        8080,
    ))
    .unwrap();
    
    println!("accepting: listener");
    loop {
        match listener.accept().await {
            Ok((stream, addr)) => executor::spawn(serve_connection(stream, addr)),
            Err(e) => println!("accept error: {}", e),
        }
        println!("accept");
    }
}

// echo服务:
// 从stream读取一个u32值, 然后再返回该u32值
async fn serve_connection(mut stream: tcpstream::TcpStream, addr: SocketAddr) {
    let mut buf: [u8; 4] = [0; 4];
    println!("incoming connection: {}", addr);
    loop {
        stream.read(&mut buf).await.unwrap();
        // unsafe {
        //     let mut rdr = Cursor::new(buf);
        //     println!("receive from client stream: {}", rdr.read_u32::<BigEndian>().unwrap());
        // }
        stream.write(&mut buf).await.unwrap();
    }
}
