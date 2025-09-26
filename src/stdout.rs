use libc::{STDOUT_FILENO, usleep, write};
use std::{
    collections::VecDeque,
    ffi::c_void,
    sync::{Condvar, Mutex, atomic::Ordering},
};

use crate::term::TERM_QUIT;

pub fn print(bytes: &[u8]) -> isize {
    unsafe { write(STDOUT_FILENO, bytes.as_ptr() as *const c_void, bytes.len()) }
}

static mut STDOUT_BUF: Mutex<VecDeque<Vec<u8>>> = Mutex::new(VecDeque::new());
static mut STDOUT_SIG: Condvar = Condvar::new();

#[allow(static_mut_refs)]
pub fn output_main() {
    let mut buf: Option<Vec<u8>> = None;
    let mut pos = 0;
    while TERM_QUIT.load(Ordering::SeqCst) == false {
        if buf.is_none() || pos >= buf.as_ref().unwrap().len() {
            let _ = buf.take();
            let mut lock = unsafe { STDOUT_BUF.lock().unwrap() };
            while lock.len() == 0 && TERM_QUIT.load(Ordering::SeqCst) == false {
                lock = unsafe { STDOUT_SIG.wait(lock).unwrap() };
            }
            if TERM_QUIT.load(Ordering::SeqCst) != false {
                break;
            }
            buf = lock.pop_front();
            pos = 0;
            continue;
        }
        let n = print(&buf.as_ref().unwrap()[pos..]);
        if n <= 0 {
            unsafe { usleep(10000) };
            break;
        }
        pos += n as usize;
    }
}

#[allow(static_mut_refs)]
pub fn notify_quit() {
    unsafe { STDOUT_SIG.notify_all() };
}

#[allow(static_mut_refs)]
pub fn pend_print(data: Vec<u8>) {
    unsafe {
        let mut lock = STDOUT_BUF.lock().unwrap();
        lock.push_back(data);
    };
    unsafe { STDOUT_SIG.notify_all() };
}

#[allow(static_mut_refs)]
pub fn pending_frames() -> usize {
    let lock = unsafe { STDOUT_BUF.lock().unwrap() };
    lock.len()
}

#[allow(static_mut_refs)]
pub fn remove_pending_frames() {
    unsafe {
        let mut lock = STDOUT_BUF.lock().unwrap();
        lock.clear();
    };
}
