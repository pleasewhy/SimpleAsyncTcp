use crate::net::tcpstream;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

pub(crate) fn write<'a>(writer: &'a mut tcpstream::TcpStream, buf: &'a mut [u8]) -> AsyncWrite<'a> {
    AsyncWrite {
        writer: writer,
        buf: buf,
    }
}

pub struct AsyncWrite<'a> {
    writer: &'a mut tcpstream::TcpStream,
    buf: &'a mut [u8],
}

impl Future for AsyncWrite<'_> {
    type Output = std::io::Result<usize>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<usize>> {
        let me = &mut *self;
        Pin::new(&mut *me.writer).poll_write(cx, me.buf)
    }
}
