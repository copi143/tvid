use std::sync::atomic::{AtomicBool, Ordering};

use parking_lot::Mutex;

use crate::{
    ffmpeg,
    stdin::{self, Key},
};

pub struct Playlist {
    items: Vec<String>,
    pos: usize,
    looping: bool,
    setnext: Option<usize>,
}

impl Playlist {
    pub const fn new() -> Self {
        Self {
            items: Vec::new(),
            pos: 0,
            looping: false,
            setnext: Some(0),
        }
    }

    pub fn clear(&mut self) -> &mut Self {
        self.items.clear();
        self.pos = 0;
        self
    }

    pub fn push(&mut self, path: &str) -> &mut Self {
        self.items.push(path.to_string());
        self
    }

    pub fn extend(&mut self, paths: Vec<String>) -> &mut Self {
        self.items.extend(paths);
        self
    }

    pub fn push_and_setnext(&mut self, path: &str) -> &mut Self {
        self.items.push(path.to_string());
        self.setnext(self.items.len() - 1);
        self
    }

    pub fn get_items(&self) -> &Vec<String> {
        &self.items
    }

    pub fn get_pos(&self) -> usize {
        self.pos
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn get_looping(&self) -> bool {
        self.looping
    }

    pub fn set_looping(&mut self, looping: bool) -> &mut Self {
        self.looping = looping;
        self
    }

    pub fn setnext(&mut self, index: usize) -> &mut Self {
        if index < self.items.len() {
            self.setnext = Some(index);
        }
        self
    }

    pub fn current(&self) -> Option<&String> {
        if self.items.len() == 0 || self.pos >= self.items.len() {
            return None;
        }
        Some(&self.items[self.pos])
    }

    pub fn next(&mut self) -> Option<&String> {
        if self.items.len() == 0 {
            return None;
        }
        if let Some(next) = self.setnext {
            self.setnext = None;
            self.pos = next;
            return Some(&self.items[self.pos]);
        }
        self.pos += 1;
        if self.pos >= self.items.len() {
            if self.looping {
                self.pos = 0;
            } else {
                self.pos = self.items.len();
                return None;
            }
        }
        Some(&self.items[self.pos])
    }

    pub fn prev(&mut self) -> Option<&String> {
        if self.items.len() == 0 {
            return None;
        }
        if let Some(next) = self.setnext {
            self.setnext = None;
            self.pos = next;
            return Some(&self.items[self.pos]);
        }
        if self.pos == 0 {
            if self.looping {
                self.pos = self.items.len() - 1;
            }
        } else {
            self.pos -= 1;
        }
        Some(&self.items[self.pos])
    }
}

pub static PLAYLIST: Mutex<Playlist> = Mutex::new(Playlist::new());
pub static SHOW_PLAYLIST: AtomicBool = AtomicBool::new(false);
pub static PLAYLIST_SELECTED_INDEX: Mutex<isize> = Mutex::new(-1);

pub fn toggle_show_playlist() {
    SHOW_PLAYLIST.fetch_xor(true, Ordering::SeqCst);
    *PLAYLIST_SELECTED_INDEX.lock() = -1;
}

pub fn register_keypress_callbacks() {
    stdin::register_keypress_callback(Key::Normal('q'), |_| {
        if !SHOW_PLAYLIST.load(Ordering::SeqCst) {
            return false;
        }
        SHOW_PLAYLIST.store(false, Ordering::SeqCst);
        true
    });

    let cb = |_| {
        if !SHOW_PLAYLIST.load(Ordering::SeqCst) {
            return false;
        }
        let index = *PLAYLIST_SELECTED_INDEX.lock();
        if index >= 0 {
            PLAYLIST.lock().setnext(index as usize);
            SHOW_PLAYLIST.store(false, Ordering::SeqCst);
            ffmpeg::notify_quit();
        }
        true
    };
    stdin::register_keypress_callback(Key::Normal(' '), cb);
    stdin::register_keypress_callback(Key::Enter, cb);

    let cb = |_| {
        if !SHOW_PLAYLIST.load(Ordering::SeqCst) {
            return false;
        }
        let len = PLAYLIST.lock().len() as isize;
        let mut lock = PLAYLIST_SELECTED_INDEX.lock();
        if *lock >= 0 {
            *lock = (*lock - 1).clamp(0, len - 1);
        } else {
            *lock = (PLAYLIST.lock().get_pos() as isize - 1).clamp(0, len - 1);
        }
        true
    };
    stdin::register_keypress_callback(Key::Normal('w'), cb);
    stdin::register_keypress_callback(Key::Up, cb);

    let cb = |_| {
        if !SHOW_PLAYLIST.load(Ordering::SeqCst) {
            return false;
        }
        let len = PLAYLIST.lock().len() as isize;
        let mut lock = PLAYLIST_SELECTED_INDEX.lock();
        if *lock >= 0 {
            *lock = (*lock + 1).clamp(0, len - 1);
        } else {
            *lock = (PLAYLIST.lock().get_pos() as isize + 1).clamp(0, len - 1);
        }
        true
    };
    stdin::register_keypress_callback(Key::Normal('s'), cb);
    stdin::register_keypress_callback(Key::Down, cb);

    stdin::register_keypress_callback(Key::Normal('a'), |_| SHOW_PLAYLIST.load(Ordering::SeqCst));
    stdin::register_keypress_callback(Key::Left, |_| SHOW_PLAYLIST.load(Ordering::SeqCst));
    stdin::register_keypress_callback(Key::Normal('d'), |_| SHOW_PLAYLIST.load(Ordering::SeqCst));
    stdin::register_keypress_callback(Key::Right, |_| SHOW_PLAYLIST.load(Ordering::SeqCst));
}
