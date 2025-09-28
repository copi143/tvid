use std::{
    collections::VecDeque,
    ffi::c_void,
    sync::{Condvar, Mutex, atomic::Ordering},
};

use crate::term::TERM_QUIT;

#[cfg(unix)]
pub fn print(bytes: &[u8]) -> isize {
    use libc::STDOUT_FILENO;
    unsafe { libc::write(STDOUT_FILENO, bytes.as_ptr() as *const c_void, bytes.len()) }
}

#[cfg(windows)]
pub fn print(bytes: &[u8]) -> isize {
    use winapi::shared::minwindef::DWORD;
    use winapi::um::consoleapi::WriteConsoleA;
    use winapi::um::processenv::GetStdHandle;
    use winapi::um::winbase::STD_OUTPUT_HANDLE;
    unsafe {
        let handle = GetStdHandle(STD_OUTPUT_HANDLE);
        let mut written = 0u32;
        let res = WriteConsoleA(
            handle,
            bytes.as_ptr() as *const c_void,
            bytes.len() as DWORD,
            &mut written,
            std::ptr::null_mut(),
        );
        if res == 0 { -1 } else { written as isize }
    }
}

static STDOUT_BUF: Mutex<VecDeque<Vec<u8>>> = Mutex::new(VecDeque::new());
static STDOUT_SIG: Condvar = Condvar::new();

pub fn output_main() {
    let mut buf: Option<Vec<u8>> = None;
    let mut pos = 0;
    while TERM_QUIT.load(Ordering::SeqCst) == false {
        if buf.is_none() || pos >= buf.as_ref().unwrap().len() {
            let _ = buf.take();
            let mut lock = STDOUT_BUF.lock().unwrap();
            while lock.len() == 0 && TERM_QUIT.load(Ordering::SeqCst) == false {
                lock = STDOUT_SIG.wait(lock).unwrap();
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
            std::thread::sleep(std::time::Duration::from_millis(50));
            break;
        }
        pos += n as usize;
    }
}

pub fn notify_quit() {
    STDOUT_SIG.notify_all();
}

pub fn pend_print(data: Vec<u8>) {
    let mut lock = STDOUT_BUF.lock().unwrap();
    lock.push_back(data);
    STDOUT_SIG.notify_all();
}

pub fn pending_frames() -> usize {
    let lock = STDOUT_BUF.lock().unwrap();
    lock.len()
}

pub fn remove_pending_frames() {
    let mut lock = STDOUT_BUF.lock().unwrap();
    lock.clear();
}
