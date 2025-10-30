use parking_lot::{Condvar, Mutex};
use std::collections::VecDeque;
use std::ffi::c_void;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use crate::statistics::set_output_time;
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
    while TERM_QUIT.load(Ordering::SeqCst) == false {
        let buf = {
            let mut lock = STDOUT_BUF.lock();
            while lock.len() == 0 && TERM_QUIT.load(Ordering::SeqCst) == false {
                STDOUT_SIG.wait(&mut lock);
            }
            if TERM_QUIT.load(Ordering::SeqCst) {
                break;
            }
            lock.pop_front().unwrap()
        };

        let instant = Instant::now();
        let mut pos = 0;
        while pos < buf.len() {
            let n = print(&buf[pos..]);
            if n <= 0 {
                std::thread::sleep(Duration::from_millis(50));
                break;
            }
            pos += n as usize;
        }
        set_output_time(instant.elapsed());
    }
}

pub fn notify_quit() {
    STDOUT_SIG.notify_all();
}

pub fn pend_print(data: Vec<u8>) {
    let mut lock = STDOUT_BUF.lock();
    lock.push_back(data);
    STDOUT_SIG.notify_all();
}

pub fn pending_frames() -> usize {
    let lock = STDOUT_BUF.lock();
    lock.len()
}

pub fn remove_pending_frames() {
    let mut lock = STDOUT_BUF.lock();
    lock.clear();
}
