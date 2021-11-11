use crate::net::tcpstream;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

pub(crate) fn read<'a>(reader: &'a mut tcpstream::TcpStream, buf: &'a mut [u8]) -> AsyncRead<'a> {
    AsyncRead {
        reader: reader,
        buf: buf,
    }
}

pub struct AsyncRead<'a> {
    reader: &'a mut tcpstream::TcpStream,
    buf: &'a mut [u8],
}

impl Future for AsyncRead<'_> {
    type Output = std::io::Result<usize>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<usize>> {
        let me = &mut *self;
        Pin::new(&mut *me.reader).poll_read(cx, me.buf)
    }
}
