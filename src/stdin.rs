use anyhow::Result;
use libc::{STDIN_FILENO, read};
use std::{
    collections::BTreeMap,
    ffi::c_void,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use crate::term::TERM_QUIT;

pub fn scan(bytes: &mut [u8]) -> isize {
    unsafe { read(STDIN_FILENO, bytes.as_mut_ptr() as *mut c_void, bytes.len()) }
}

static STDIN_QUIT: AtomicBool = AtomicBool::new(false);

static mut STDIN_BUF: [u8; 4096] = [0; 4096];
static mut STDIN_POS: usize = 0;
static mut STDIN_LEN: usize = 0;

#[allow(static_mut_refs)]
pub fn getc() -> Result<u8> {
    unsafe {
        if STDIN_POS < STDIN_LEN {
            let c = *STDIN_BUF.get_unchecked(STDIN_POS);
            STDIN_POS += 1;
            Ok(c)
        } else {
            let mut n = scan(&mut STDIN_BUF);
            while n == 0 && STDIN_QUIT.load(Ordering::SeqCst) == false {
                std::thread::sleep(Duration::from_millis(10));
                n = scan(&mut STDIN_BUF);
            }
            if STDIN_QUIT.load(Ordering::SeqCst) {
                return Err(anyhow::anyhow!("stdin quit"));
            }
            if n > 0 {
                STDIN_POS = 1;
                STDIN_LEN = n as usize;
                Ok(STDIN_BUF[0])
            } else {
                Err(anyhow::anyhow!("failed to read from stdin"))
            }
        }
    }
}

static mut KEYPRESS_CALLBACKS: BTreeMap<u8, Vec<Box<dyn Fn(u8) + Send + Sync>>> = BTreeMap::new();

#[allow(static_mut_refs)]
pub fn register_keypress_callback<F: Fn(u8) + Send + Sync + 'static>(c: u8, f: F) {
    let c = if b'a' <= c && c <= b'z' {
        c - b'a' + b'A'
    } else {
        c
    };
    unsafe {
        KEYPRESS_CALLBACKS
            .entry(c)
            .or_insert_with(Vec::new)
            .push(Box::new(f));
    };
}

#[allow(static_mut_refs)]
pub fn input_main() {
    while TERM_QUIT.load(Ordering::SeqCst) == false {
        let Ok(c) = getc() else {
            return;
        };
        match c {
            _ => unsafe {
                let c = if b'a' <= c && c <= b'z' {
                    c - b'a' + b'A'
                } else {
                    c
                };
                if let Some(callbacks) = KEYPRESS_CALLBACKS.get(&c) {
                    for f in callbacks {
                        f(c);
                    }
                }
            },
        }
    }
}

#[allow(static_mut_refs)]
pub fn notify_quit() {
    STDIN_QUIT.store(true, Ordering::SeqCst);
}
