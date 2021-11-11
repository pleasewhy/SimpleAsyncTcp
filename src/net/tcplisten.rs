extern crate alloc;

use crate::net::poll_fn::poll_fn;
use lazy_static::__Deref;
use mio::net;

use crate::net::{tcpstream, Polled, EVENT_STATE_POOL};
use std::io::{self, ErrorKind};
use std::net::SocketAddr;
use std::sync::atomic::Ordering;
use std::task::{Context, Poll};

pub struct TcpListener {
    io: Polled<net::TcpListener>,
}

pub fn bind_addr(addr: std::net::SocketAddr) -> std::io::Result<TcpListener> {
    let listener = net::TcpListener::bind(&addr)?;
    TcpListener::new(listener)
}

impl TcpListener {
    pub fn new(io: mio::net::TcpListener) -> std::io::Result<TcpListener> {
        Ok(TcpListener {
            io: Polled::new(io).unwrap(),
        })
    }

    pub async fn accept(
        &mut self,
    ) -> std::io::Result<(tcpstream::TcpStream, std::net::SocketAddr)> {
        let ret = poll_fn(|cx| self.poll_accept(cx)).await;
        return ret;
    }

    pub fn poll_accept(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<io::Result<(tcpstream::TcpStream, std::net::SocketAddr)>> {
        let (io, addr) = match self.poll_accept_std(cx) {
            Poll::Ready(r) => r,
            Poll::Pending => return Poll::Pending,
        }?;
        let io = mio::net::TcpStream::from_stream(io)?;
        let io = tcpstream::TcpStream::new(io)?;
        return Poll::Ready(Ok((io, addr)));
    }

    pub fn poll_accept_std(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<io::Result<(std::net::TcpStream, SocketAddr)>> {
        match self.io.poll_read_ready(cx) {
            Poll::Pending => return Poll::Pending,
            _ => (),
        };
        match self.io.io.as_ref().unwrap().accept_std() {
            Ok(pair) => {
                Poll::Ready(Ok(pair))
            }
            Err(e) => {
                // 若没有新的连接, 则将listen.readiness_read设置为false
                // 这里会有安全问题: 若在accept_std()和store(false)之间来了一个connection,
                // 则该connection只能在在下一次connection到来是才会被处理
                // if let io::ErrorKind::NotConnected = e.kind() {
                let event_state_pool = EVENT_STATE_POOL.lock().unwrap();
                if let Some(event_state) = event_state_pool.get(&self.io.token.0) {
                    event_state.readiness_read.store(false, Ordering::SeqCst);
                }
                // }
                println!("err {:?}", e.kind());
                Poll::Ready(Err(e))
            }
        }
    }
}
