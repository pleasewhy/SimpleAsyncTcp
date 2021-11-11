pub mod async_read;
pub mod async_write;
pub mod poll_fn;
pub mod tcplisten;
pub mod tcpstream;

use lazy_static::*;
use mio::{Evented, Events, Ready, Token};

use std::collections::HashMap;
use std::io;
use std::ops::DerefMut;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::Duration;

lazy_static! {
    pub static ref GLOBAL_POLL: Arc<mio::Poll> = Arc::new(mio::Poll::new().unwrap());
    pub static ref EVENT_STATE_POOL: Arc<Mutex<HashMap<usize, EventState>>> =
        Arc::new(Mutex::new(HashMap::new()));
    pub static ref TOKEN_ALLOCTOR: AtomicUsize = AtomicUsize::from(0);
}

struct Polled<E> {
    io: Option<E>,
    token: Token,
}

impl<E: Evented> Polled<E> {
    pub fn new(source: E) -> std::io::Result<Self> {
        let token = add_source(&source, Ready::all()).unwrap();
        Ok(Polled {
            io: Some(source),
            token: token,
        })
    }

    pub fn poll_read_ready(&self, cx: &mut Context<'_>) -> Poll<io::Result<mio::Ready>> {
        let mut event_state_pool = EVENT_STATE_POOL.lock().unwrap();
        let state = event_state_pool.get_mut(&self.token.0).unwrap();
        let waker_ptr = &mut (*state).waker;
        if (*waker_ptr).is_none() {
            *waker_ptr = Some(cx.waker().clone());
        }

        if !state.readiness_read.load(Ordering::Acquire) {
            return Poll::Pending;
        }
        Poll::Ready(Ok(Ready::readable()))
    }
}

pub struct EventState {
    readiness_read: AtomicBool,
    readiness_write: AtomicBool,
    waker: Option<std::task::Waker>,
}

impl EventState {
    fn register_waker(&self) {}
}

/// 将一个Evented handle注册到GLOBAL_POLL中, 并返回一个token.
/// 调用者可以通过该token在EVENT_STATE_POOL查询其监听的时间是否
/// 就绪
fn add_source(source: &dyn Evented, ready: mio::Ready) -> io::Result<Token> {
    let token = Token(TOKEN_ALLOCTOR.fetch_add(1, Ordering::AcqRel));
    let mut event_state_pool = EVENT_STATE_POOL.lock().unwrap();
    event_state_pool.deref_mut().insert(
        token.0,
        EventState {
            readiness_read: AtomicBool::from(false),
            readiness_write: AtomicBool::from(false),
            waker: None,
        },
    );
    GLOBAL_POLL.register(source, token, ready, mio::PollOpt::edge())?;
    return Ok(token);
}

pub fn init() -> Result<(), std::io::Error> {
    let poll = Arc::clone(&GLOBAL_POLL);
    std::thread::spawn(move || {
        let mut es = Events::with_capacity(100);
        loop {
            let res = poll.poll(&mut es, Some(Duration::from_millis(500)));
            for e in &es {
                let event_state_pool = EVENT_STATE_POOL.lock().unwrap();
                if let Some(event_state) = event_state_pool.get(&e.token().0) {
                    if e.readiness().is_readable() {
                        event_state.readiness_read.store(true, Ordering::SeqCst);
                    }

                    if e.readiness().is_writable() {
                        event_state.readiness_write.store(true, Ordering::SeqCst);
                    }

                    if event_state.waker.is_some() {
                        let w = event_state.waker.as_ref().unwrap();
                        w.wake_by_ref();
                    }
                }
            }
        }
    });
    Ok(())
}
