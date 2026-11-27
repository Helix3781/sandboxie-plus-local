//! Raw INI reader/writer that preserves unknown lines and comments.
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct RawIni {
    pub path: PathBuf,
    pub lines: Vec<String>,
    pub bom: bool,
    pub utf16: bool,
}

impl RawIni {
    pub fn load(path: &Path) -> io::Result<Self> {
        let bytes = fs::read(path)?;
        let utf16_bom = bytes.starts_with(&[0xFF, 0xFE]);
        let (bom, utf16, text) = if utf16_bom {
            (true, true, decode_utf16(&bytes[2..]))
        } else if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
            (false, false, String::from_utf8_lossy(&bytes[3..]).to_string())
        } else if looks_utf16(&bytes) {
            (false, true, decode_utf16(&bytes))
        } else {
            (false, false, String::from_utf8_lossy(&bytes).to_string())
        };
        Ok(RawIni { path: path.to_path_buf(), lines: split_lines(&text), bom, utf16 })
    }

    pub fn load_or_empty(path: &Path) -> Self {
        Self::load(path).unwrap_or_else(|_| RawIni {
            path: path.to_path_buf(),
            lines: Vec::new(),
            bom: false,
            utf16: false,
        })
    }

    fn section_range(&self, section: &str) -> Option<(usize, usize)> {
        let want = format!("[{}]", section);
        let mut start = None;
        for (i, line) in self.lines.iter().enumerate() {
            let t = line.trim();
            if t.starts_with('[') && t.ends_with(']') {
                if let Some(s) = start {
                    return Some((s, i));
                }
                if t.eq_ignore_ascii_case(&want) {
                    start = Some(i);
                }
            }
        }
        start.map(|s| (s, self.lines.len()))
    }

    pub fn sections(&self) -> Vec<String> {
        let mut out = Vec::new();
        for line in &self.lines {
            let t = line.trim();
            if t.starts_with('[') && t.ends_with(']') {
                out.push(t[1..t.len() - 1].to_string());
            }
        }
        out
    }

    pub fn get(&self, section: &str, key: &str) -> Option<String> {
        let (start, end) = self.section_range(section)?;
        for line in &self.lines[start + 1..end] {
            if let Some((k, v)) = split_kv(line) {
                if k.eq_ignore_ascii_case(key) {
                    return Some(v);
                }
            }
        }
        None
    }

    /// All `key=value` entries of a section, in file order.
    pub fn entries(&self, section: &str) -> Vec<(String, String)> {
        let mut out = Vec::new();
        if let Some((start, end)) = self.section_range(section) {
            for line in &self.lines[start + 1..end] {
                if let Some((k, v)) = split_kv(line) {
                    out.push((k, v));
                }
            }
        }
        out
    }

    pub fn set(&mut self, section: &str, key: &str, value: &str) {
        if let Some((start, end)) = self.section_range(section) {
            for i in start + 1..end {
                if let Some((k, _)) = split_kv(&self.lines[i]) {
                    if k.eq_ignore_ascii_case(key) {
                        self.lines[i] = format!("{}={}", key, value);
                        return;
                    }
                }
            }
            self.lines.insert(end, format!("{}={}", key, value));
            return;
        }
        if !self.lines.is_empty() && !self.lines.last().unwrap().trim().is_empty() {
            self.lines.push(String::new());
        }
        self.lines.push(format!("[{}]", section));
        self.lines.push(format!("{}={}", key, value));
    }

    /// Append one more `key=value` line for a multi-valued setting,
    /// keeping any existing lines that share the same key.
    pub fn append(&mut self, section: &str, key: &str, value: &str) {
        if let Some((start, end)) = self.section_range(section) {
            let mut last: Option<usize> = None;
            for i in start + 1..end {
                if let Some((k, _)) = split_kv(&self.lines[i]) {
                    if k.eq_ignore_ascii_case(key) {
                        last = Some(i);
                    }
                }
            }
            let at = match last {
                Some(i) => i + 1,
                None => end,
            };
            self.lines.insert(at, format!("{}={}", key, value));
            return;
        }
        if !self.lines.is_empty() && !self.lines.last().unwrap().trim().is_empty() {
            self.lines.push(String::new());
        }
        self.lines.push(format!("[{}]", section));
        self.lines.push(format!("{}={}", key, value));
    }

    pub fn remove_section(&mut self, section: &str) {
        if let Some((start, end)) = self.section_range(section) {
            let end = if end < self.lines.len() { end } else { self.lines.len() };
            self.lines.drain(start..end);
        }
    }

    pub fn ensure_section(&mut self, section: &str) {
        if self.section_range(section).is_none() {
            if !self.lines.is_empty() && !self.lines.last().unwrap().trim().is_empty() {
                self.lines.push(String::new());
            }
            self.lines.push(format!("[{}]", section));
        }
    }

    pub fn save(&self) -> io::Result<()> {
        let mut text = String::new();
        if self.bom && self.utf16 {
            text.push('\u{feff}');
        }
        text.push_str(&self.lines.join("\r\n"));
        text.push_str("\r\n");
        if self.utf16 {
            fs::write(&self.path, encode_utf16(&text))
        } else {
            fs::write(&self.path, text.as_bytes())
        }
    }
}

pub fn split_kv(line: &str) -> Option<(String, String)> {
    let t = line.trim();
    if t.is_empty() || t.starts_with(';') || t.starts_with('#') || t.starts_with('[') {
        return None;
    }
    let idx = t.find('=')?;
    Some((t[..idx].trim().to_string(), t[idx + 1..].trim().to_string()))
}

fn split_lines(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = text
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l).to_string())
        .collect();
    while lines.last().map(|l| l.is_empty()).unwrap_or(false) {
        lines.pop();
    }
    lines
}

/// UTF-16LE text has a NUL byte in (nearly) every high byte of an ASCII char.
pub fn looks_utf16(bytes: &[u8]) -> bool {
    if bytes.len() < 4 {
        return false;
    }
    let sample = &bytes[..bytes.len().min(4096)];
    let zeros = sample.iter().filter(|b| **b == 0).count();
    zeros * 5 > sample.len() // >20% zero bytes
}

pub fn decode_utf16(bytes: &[u8]) -> String {    let mut units = Vec::with_capacity(bytes.len() / 2);
    let mut i = 0;
    while i + 1 < bytes.len() {
        units.push(u16::from_le_bytes([bytes[i], bytes[i + 1]]));
        i += 2;
    }
    if i < bytes.len() {
        // odd trailing byte: treat as latin-1 fallback
        let mut s = String::from_utf16_lossy(&units);
        s.push(bytes[i] as char);
        return s;
    }
    String::from_utf16_lossy(&units)
}

pub fn encode_utf16(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() * 2);
    for unit in text.encode_utf16() {
        out.extend_from_slice(&unit.to_le_bytes());
    }
    out
}
