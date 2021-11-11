extern crate alloc;

use crate::waker_page::{SharedWaker, WakerPage, WakerPageRef, WAKER_PAGE_SIZE};
use lazy_static::*;
use {
    core::pin::Pin,
    futures::{
        future::FutureExt,
        task::Waker,
        task::ArcWake,
    },
    std::{
        future::Future,
        sync::{Arc, Mutex},
        task::{Context, Poll},
    },
};

use bit_iter::BitIter;
use libc;
use nix::sys::signal::{signal, SigHandler, Signal};
use std::cell::RefCell;
use unicycle::pin_slab::PinSlab;

enum TaskState {
    _BLOCKED,
    RUNNABLE,
    _RUNNING,
}

/// 一个可以重新安排自己被 `Executor` 调用的 `future`.
pub struct Task {
    /// 正在运行的 `future` 应该被推进到运行完成.
    ///
    /// 这个 `Mutex` 不是必要的, 因为我们一次只有一个线程
    /// 执行任务，但是，Rust不够聪明，没有办法知道 `future`
    /// 只会在一个线程中发生变化，所以我们需要 `Mutex` 来
    /// 让Rust知道我们保证了跨线程之间的安全性.
    future: Mutex<Pin<Box<dyn Future<Output = ()> + Send>>>,
    state: Mutex<TaskState>,
    _priority: u8,
}

impl ArcWake for Task {
    fn wake_by_ref(arc_self: &Arc<Self>) {
        // 将state设置为RUNNABLE以便executor运行
        let mut task_state = arc_self.state.lock().unwrap();
        *task_state = TaskState::RUNNABLE;
    }
}

impl Future for Task {
    type Output = ();
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let mut f = self.future.lock().unwrap();
        return f.poll_unpin(cx);
    }
}

impl Task {
    pub fn _is_runnable(&self) -> bool {
        let task_state = self.state.lock().unwrap();
        if let TaskState::RUNNABLE = *task_state {
            true
        } else {
            false
        }
    }

    pub fn _block(&self) {
        let mut task_state = self.state.lock().unwrap();
        *task_state = TaskState::_BLOCKED;
    }
}

pub struct Inner<F: Future<Output = ()> + Unpin> {
    slab: PinSlab<F>,
    root_waker: SharedWaker,
    pages: Vec<WakerPageRef>,
}

impl<F: Future<Output = ()> + Unpin> Inner<F> {
    /// Our pages hold 64 contiguous future wakers, so we can do simple arithmetic to access the
    /// correct page as well as the index within page.
    /// Given the `key` representing a future, return a reference to that page, `WakerPageRef`. And
    /// the index _within_ that page (usize).
    fn page(&self, key: u64) -> (&WakerPageRef, usize) {
        let key = key as usize;
        let (page_ix, subpage_ix) = (key / WAKER_PAGE_SIZE, key % WAKER_PAGE_SIZE);
        (&self.pages[page_ix], subpage_ix)
    }

    /// Insert a future into our scheduler returning an integer key representing this future. This
    /// key is used to index into the slab for accessing the future.
    fn insert(&mut self, future: F) -> u64 {
        let key = self.slab.insert(future);

        // Add a new page to hold this future's status if the current page is filled.
        while key >= self.pages.len() * WAKER_PAGE_SIZE {
            self.pages.push(WakerPage::new(self.root_waker.clone()));
        }
        let (page, subpage_ix) = self.page(key as u64);
        page.initialize(subpage_ix);
        key as u64
    }
}


pub struct Executor<F: Future<Output = ()> + Unpin> {
    inners: Vec<RefCell<Inner<F>>>,
}

impl<F: Future<Output = ()> + Unpin> Executor<F> {
    pub fn new() -> Self {
        let mut executor = Executor { inners: vec![] };
        for _ in 0..MAX_PRIORITY {
            let inner = Inner {
                slab: PinSlab::new(),
                pages: vec![],
                root_waker: SharedWaker::new(),
            };
            executor.inners.push(RefCell::new(inner));
        }
        return executor;
    }
    pub fn run(&self) {
        loop {
            for priority in 0..16 {
                let mut inner = self.inners[priority].borrow_mut();
                for page_idx in 0..inner.pages.len() {
                    let (notified, _dropped) = {
                        let page = &mut inner.pages[page_idx];
                        (page.take_notified(), page.take_dropped())
                    };
                    if notified != 0 {
                        for subpage_idx in BitIter::from(notified) {
                            // task的idx
                            let idx = page_idx * WAKER_PAGE_SIZE + subpage_idx;
                            let waker = unsafe {
                                Waker::from_raw((&inner.pages[page_idx]).raw_waker(subpage_idx))
                            };
                            let mut cx = Context::from_waker(&waker);
                            let pinned_ref = inner.slab.get_pin_mut(idx).unwrap();
                            let pinned_ptr =
                                unsafe { Pin::into_inner_unchecked(pinned_ref) as *mut _ };
                            let pinned_ref = unsafe { Pin::new_unchecked(&mut *pinned_ptr) };
                            drop(inner); // 本次运行的coroutine可能会用到GLOBAL_EXCUTOR.inner(e.g. spawn())
                            let ret = { Future::poll(pinned_ref, &mut cx) };
                            inner = self.inners[priority].borrow_mut();
                            match ret {
                                Poll::Ready(()) => inner.pages[page_idx].mark_dropped(subpage_idx),
                                Poll::Pending => (),
                            }
                        }
                    }
                }
            }
        }
    }

    fn add_task(&self, priority: usize, future: F) -> u64 {
        assert!(priority < MAX_PRIORITY);
        let mut inner = self.inners[priority as usize].borrow_mut();
        let key = inner.insert(future);
        println!("add_task: key={}", key);
        let (_page, _) = inner.page(key);
        key
    }
}

unsafe impl Send for Executor<Task> {}
unsafe impl Sync for Executor<Task> {}

lazy_static! {
    pub static ref GLOBAL_EXECUTOR: Executor<Task> = Executor::new();
}

const DEFAULT_PRIORITY: usize = 4;
const MAX_PRIORITY: usize = 32;

pub fn spawn(future: impl Future<Output = ()> + Send + 'static) {
    return priority_spawn(future, DEFAULT_PRIORITY);
}

pub fn priority_spawn(future: impl Future<Output = ()> + Send + 'static, priority: usize) {
    let bf = future.boxed();
    let future = Mutex::from(bf);
    let state = Mutex::from(TaskState::RUNNABLE);
    let task = Task {
        future,
        state,
        _priority: priority as u8,
    };
    GLOBAL_EXECUTOR.add_task(priority, task);
}

extern "C" fn handle_sigint(signal: libc::c_int) {
    println!("{}", signal);
}

pub fn run() {
    unsafe { signal(Signal::SIGINT, SigHandler::Handler(handle_sigint)) }
        .expect("Error install SIGINT hander");
    GLOBAL_EXECUTOR.run();
}
