use std::sync::Mutex;

pub struct Playlist {
    items: Vec<String>,
    pos: usize,
    looping: bool,
    first_run: bool,
}

impl Playlist {
    pub const fn new() -> Self {
        Self {
            items: Vec::new(),
            pos: 0,
            looping: false,
            first_run: true,
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

    pub fn get_items(&self) -> &Vec<String> {
        &self.items
    }

    pub fn get_pos(&self) -> usize {
        self.pos
    }

    pub fn get_looping(&self) -> bool {
        self.looping
    }

    pub fn set_looping(&mut self, looping: bool) -> &mut Self {
        self.looping = looping;
        self
    }

    pub fn current(&self) -> Option<String> {
        if self.items.len() == 0 || self.pos >= self.items.len() {
            return None;
        }
        Some(self.items[self.pos].clone())
    }

    pub fn next(&mut self) -> Option<String> {
        if self.items.len() == 0 {
            return None;
        }
        if self.first_run {
            self.first_run = false;
            return Some(self.items[self.pos].clone());
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
        Some(self.items[self.pos].clone())
    }

    pub fn prev(&mut self) -> Option<String> {
        if self.items.len() == 0 {
            return None;
        }
        if self.first_run {
            self.first_run = false;
            return Some(self.items[self.pos].clone());
        }
        if self.pos == 0 {
            if self.looping {
                self.pos = self.items.len() - 1;
            }
        } else {
            self.pos -= 1;
        }
        Some(self.items[self.pos].clone())
    }
}

pub static PLAYLIST: Mutex<Playlist> = Mutex::new(Playlist::new());
