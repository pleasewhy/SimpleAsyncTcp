use std::process::Command;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::delay_for;

fn startup_server() {
    std::thread::spawn(move || {
        let mut path = std::env::current_exe().expect("Could not get current test executable path");
        path.pop();
        path.pop();
        path.push("async_test");
        println!("path {}", path.as_path().to_str().unwrap());
        let mut cmd = Command::new(path);
        let child = cmd.spawn().expect(&format!("run server failed.",));
    });
    std::thread::sleep(Duration::from_secs(2));
}

#[tokio::test]
async fn test_accept() {
    startup_server();
    for i in 0..5 {
        tokio::spawn(client());
    }
    println!("created 5 client");
    delay_for(Duration::from_millis(10000)).await;
}

async fn client() {
    let mut tcp_stream = TcpStream::connect("127.0.0.1:8080").await.unwrap();
    println!("[client]: remote_addr={:?}", tcp_stream.peer_addr());
    for i in 1..100 {
        tcp_stream.write_u32(i).await.unwrap();
        let val = tcp_stream.read_u32().await.unwrap();
        if val != i{
            panic!("[client]: Error(request={}, response={})", i, val);
        }
        delay_for(Duration::from_millis(10)).await;
    }
    println!("[client]: over");
    delay_for(Duration::from_millis(2000)).await;
}
