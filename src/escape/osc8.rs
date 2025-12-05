pub fn begin_link(url: &str) -> String {
    let url = url
        .replace('\\', "\\\\")
        .replace(';', "\\;")
        .replace(':', "\\:");
    format!("\x1b]8;;{}\x1b\\", url)
}

pub fn end_link() -> String {
    "\x1b]8;;\x1b\\".to_string()
}

pub fn format_link(content: &str, url: &str) -> String {
    let url = url
        .replace('\\', "\\\\")
        .replace(';', "\\;")
        .replace(':', "\\:");
    format!("\x1b]8;;{}\x1b\\{}\x1b]8;;\x1b\\", url, content)
}
