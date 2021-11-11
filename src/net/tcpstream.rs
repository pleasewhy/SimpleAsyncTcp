extern crate alloc;

use lazy_static::__Deref;

use crate::net::async_read;
use crate::net::{Polled, EVENT_STATE_POOL};
use std::io::{self, Read, Write};
use std::sync::atomic::Ordering;
use std::task::{Context, Poll};

use super::async_write;

pub struct TcpStream {
    io: Polled<mio::net::TcpStream>,
}

impl TcpStream {
    pub fn new(io: mio::net::TcpStream) -> std::io::Result<TcpStream> {
        Ok(TcpStream {
            io: Polled::new(io).unwrap(),
        })
    }

    pub async fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let ret = async_read::read(self, buf).await;
        return ret;
    }

    pub fn poll_read(&mut self, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<io::Result<usize>> {
        match self.io.poll_read_ready(cx) {
            Poll::Pending => return Poll::Pending,
            _ => (),
        };
        let event_state_pool = EVENT_STATE_POOL.lock().unwrap();
        let state = event_state_pool.deref().get(&self.io.token.0);
        let r = state.unwrap().readiness_read.compare_exchange(
            true,
            false,
            Ordering::Acquire,
            Ordering::Acquire,
        );
        if r.is_err() {
            return Poll::Pending;
        }

        match self.io.io.as_ref().unwrap().read(buf) {
            Ok(r) => return Poll::Ready(Ok(r)),
            Err(e) => return Poll::Ready(Err(e)),
        }
    }

    pub async fn write(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        async_write::write(self, buf).await
    }

    pub fn poll_write(&mut self, _cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<io::Result<usize>> {
        // let event_state_pool = EVENT_STATE_POOL.lock().unwrap();
        // let state = event_state_pool.as_ref().get(&self.io.token.0);
        // let r = state.unwrap().readiness_read.compare_exchange(
        //     true,
        //     false,
        //     Ordering::Acquire,
        //     Ordering::Acquire,
        // );
        // if r.is_err() {
        //     return Poll::Pending;
        // }

        match self.io.io.as_ref().unwrap().write(buf) {
            Ok(r) => return Poll::Ready(Ok(r)),
            Err(e) => return Poll::Ready(Err(e)),
        }
    }
}
