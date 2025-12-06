use parking_lot::Mutex;
use std::collections::VecDeque;
use std::ffi::c_void;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use tokio::sync::Notify;

use crate::statistics::set_output_time;
use crate::term::TERM_QUIT;

#[cfg(unix)]
pub fn print(bytes: &[u8]) -> Option<usize> {
    use libc::STDOUT_FILENO;
    let res = unsafe { libc::write(STDOUT_FILENO, bytes.as_ptr() as *const c_void, bytes.len()) };
    if res < 0 { None } else { Some(res as usize) }
}

#[cfg(windows)]
pub fn print(bytes: &[u8]) -> Option<usize> {
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
        if res == 0 {
            None
        } else {
            Some(written as usize)
        }
    }
}

pub fn print_all(bytes: &[u8]) -> bool {
    let mut pos = 0;
    while pos < bytes.len() {
        if let Some(n) = print(&bytes[pos..]) {
            pos += n;
        } else {
            return false;
        }
    }
    true
}

static STDOUT_BUF: Mutex<VecDeque<Vec<u8>>> = Mutex::new(VecDeque::new());
static STDOUT_SIG: Notify = Notify::const_new();

pub async fn output_main() {
    while TERM_QUIT.load(Ordering::SeqCst) == false {
        let buf = {
            let mut option_buf = None;
            while TERM_QUIT.load(Ordering::SeqCst) == false {
                if let Some(buf) = STDOUT_BUF.lock().pop_front() {
                    option_buf.replace(buf);
                    break;
                }
                STDOUT_SIG.notified().await;
            }
            if let Some(buf) = option_buf {
                buf
            } else {
                break;
            }
        };

        if buf.len() == 0 {
            set_output_time(0, Duration::ZERO);
            continue;
        }

        let instant = Instant::now();
        let succ = print_all(&buf);
        set_output_time(0, instant.elapsed());

        // let terms = crate::ssh::TERMINALS
        //     .lock()
        //     .values()
        //     .cloned()
        //     .collect::<Vec<_>>();
        // for term in terms {
        //     if term.stdout(&buf).await.is_err() {
        //         term.close().await.ok();
        //     }
        // }

        if !succ {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}

pub fn notify_quit() {
    STDOUT_SIG.notify_one();
}

pub fn pend_print(data: Vec<u8>) {
    let mut lock = STDOUT_BUF.lock();
    lock.push_back(data);
    STDOUT_SIG.notify_one();
}

pub fn pending_frames() -> usize {
    let lock = STDOUT_BUF.lock();
    lock.len()
}

pub fn remove_pending_frames() {
    let mut lock = STDOUT_BUF.lock();
    lock.clear();
}
