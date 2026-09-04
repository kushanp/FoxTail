//! File tailing engine: sparse line index, live follow, encoding, truncation.

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

#[cfg(windows)]
use std::os::windows::fs::OpenOptionsExt;

const FILE_SHARE_READ: u32 = 0x0000_0001;
const FILE_SHARE_WRITE: u32 = 0x0000_0002;
const FILE_SHARE_DELETE: u32 = 0x0000_0004;

/// How often we store a byte-offset checkpoint (every Nth line).
const STRIDE: u32 = 32;
/// Bytes processed per indexing step.
const INDEX_CHUNK: usize = 2 * 1024 * 1024;
/// Tail window shown while the full-file index is still building.
const PREVIEW_BYTES: u64 = 512 * 1024;
/// Cap a single displayed line so a huge line cannot freeze the UI.
const MAX_LINE_BYTES: usize = 256 * 1024;
const MAX_DISPLAY_CHARS: usize = 16_384;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodingKind {
    Utf8,
    Utf16Le,
    Utf16Be,
    Windows1252,
}

impl EncodingKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Utf8 => "UTF-8",
            Self::Utf16Le => "UTF-16 LE",
            Self::Utf16Be => "UTF-16 BE",
            Self::Windows1252 => "ANSI (Windows-1252)",
        }
    }

    pub fn all() -> [EncodingKind; 4] {
        [Self::Utf8, Self::Utf16Le, Self::Utf16Be, Self::Windows1252]
    }

    fn encoding(self) -> &'static encoding_rs::Encoding {
        match self {
            Self::Utf8 => encoding_rs::UTF_8,
            Self::Utf16Le => encoding_rs::UTF_16LE,
            Self::Utf16Be => encoding_rs::UTF_16BE,
            Self::Windows1252 => encoding_rs::WINDOWS_1252,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct PollResult {
    pub new_lines: u64,
    pub truncated: bool,
    pub grew: bool,
}

/// Live view of a log file. Does not load the whole file into memory.
pub struct TailedFile {
    path: PathBuf,
    file: File,
    encoding: EncodingKind,
    bom_len: u64,
    file_size: u64,
    tab_width: usize,

    /// Byte offset of line 0, STRIDE, 2*STRIDE, ...
    checkpoints: Vec<u64>,
    /// Newline-terminated lines seen so far.
    complete_lines: u64,
    /// File offset of `remainder`.
    remainder_start: u64,
    /// Bytes of the current incomplete last line.
    remainder: Vec<u8>,
    index_complete: bool,

    /// Last N lines of the file, used while indexing and as a follow cache.
    preview: Vec<String>,

    line_cache: Option<(u64, Vec<String>)>,
}

impl TailedFile {
    pub fn open(path: impl AsRef<Path>, tab_width: usize) -> io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let file = open_shared(&path)?;
        let mut this = Self {
            path,
            file,
            encoding: EncodingKind::Utf8,
            bom_len: 0,
            file_size: 0,
            tab_width: tab_width.max(1).min(16),
            checkpoints: Vec::new(),
            complete_lines: 0,
            remainder_start: 0,
            remainder: Vec::new(),
            index_complete: false,
            preview: Vec::new(),
            line_cache: None,
        };
        this.detect_and_reset()?;
        this.refresh_preview()?;
        this.index_chunk(INDEX_CHUNK)?;
        Ok(this)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn file_name(&self) -> String {
        self.path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.display().to_string())
    }

    pub fn file_size(&self) -> u64 {
        self.file_size
    }

    pub fn encoding(&self) -> EncodingKind {
        self.encoding
    }

    pub fn set_encoding(&mut self, encoding: EncodingKind) -> io::Result<()> {
        if self.encoding == encoding {
            return Ok(());
        }
        self.encoding = encoding;
        self.reset_index_keep_encoding()?;
        self.refresh_preview()?;
        self.index_chunk(INDEX_CHUNK)?;
        Ok(())
    }

    pub fn set_tab_width(&mut self, tab_width: usize) {
        let tab_width = tab_width.max(1).min(16);
        if self.tab_width != tab_width {
            self.tab_width = tab_width;
            self.line_cache = None;
            let _ = self.refresh_preview();
        }
    }

    pub fn is_index_complete(&self) -> bool {
        self.index_complete
    }

    /// 0.0–1.0 while building the full-file index.
    pub fn index_progress(&self) -> f32 {
        if self.index_complete || self.file_size == 0 {
            1.0
        } else {
            let pos = self.remainder_start + self.remainder.len() as u64;
            (pos as f32 / self.file_size as f32).clamp(0.0, 1.0)
        }
    }

    /// Lines the UI can currently navigate.
    pub fn view_line_count(&self) -> u64 {
        if self.index_complete {
            self.line_count()
        } else {
            self.preview.len() as u64
        }
    }

    pub fn line_count(&self) -> u64 {
        self.complete_lines + u64::from(!self.remainder.is_empty())
    }

    pub fn reload(&mut self) -> io::Result<()> {
        self.file = open_shared(&self.path)?;
        self.detect_and_reset()?;
        self.refresh_preview()?;
        self.index_chunk(INDEX_CHUNK)?;
        Ok(())
    }

    pub fn poll(&mut self) -> io::Result<PollResult> {
        let new_size = self.file.metadata()?.len();
        let truncated = new_size < self.file_size;
        if truncated {
            self.file = open_shared(&self.path)?;
            let encoding = self.encoding;
            self.detect_and_reset()?;
            self.encoding = encoding;
            self.bom_len = bom_len_for(self.encoding, self.bom_len);
            self.remainder_start = self.bom_len;
            self.checkpoints = vec![self.bom_len];
            self.refresh_preview()?;
            self.index_chunk(INDEX_CHUNK)?;
            return Ok(PollResult {
                new_lines: self.view_line_count(),
                truncated: true,
                grew: true,
            });
        }

        let grew = new_size > self.file_size;
        self.file_size = new_size;
        let before = self.line_count();

        if !self.index_complete {
            self.index_chunk(INDEX_CHUNK)?;
            self.refresh_preview()?;
            let after = self.view_line_count();
            return Ok(PollResult {
                new_lines: after.saturating_sub(before.min(after)),
                truncated: false,
                grew,
            });
        }

        if grew {
            self.index_chunk(usize::MAX)?;
            self.line_cache = None;
        }

        let new_lines = self.line_count().saturating_sub(before);
        Ok(PollResult {
            new_lines,
            truncated: false,
            grew,
        })
    }

    pub fn read_view_lines(&mut self, start: u64, count: usize) -> Vec<String> {
        if count == 0 {
            return Vec::new();
        }
        if !self.index_complete {
            let start = start as usize;
            if start >= self.preview.len() {
                return Vec::new();
            }
            let end = (start + count).min(self.preview.len());
            return self.preview[start..end].to_vec();
        }
        self.read_lines(start, count)
    }

    pub fn read_line(&mut self, index: u64) -> String {
        self.read_view_lines(index, 1)
            .into_iter()
            .next()
            .unwrap_or_default()
    }

    fn read_lines(&mut self, start: u64, count: usize) -> Vec<String> {
        let total = self.line_count();
        if start >= total || count == 0 {
            return Vec::new();
        }
        let count = (count as u64).min(total - start) as usize;

        if let Some((cstart, lines)) = &self.line_cache {
            if start >= *cstart {
                let off = (start - *cstart) as usize;
                if off + count <= lines.len() {
                    return lines[off..off + count].to_vec();
                }
            }
        }

        let mut out = Vec::with_capacity(count);
        if let Err(err) = self.read_lines_from_index(start, count, &mut out) {
            out.push(format!("<read error: {err}>"));
        }
        self.line_cache = Some((start, out.clone()));
        out
    }

    fn read_lines_from_index(
        &mut self,
        start: u64,
        count: usize,
        out: &mut Vec<String>,
    ) -> io::Result<()> {
        let stride = STRIDE as u64;
        let cp_idx = (start / stride) as usize;
        let cp_idx = cp_idx.min(self.checkpoints.len().saturating_sub(1));
        let mut file_pos = self.checkpoints[cp_idx];
        let mut line_idx = cp_idx as u64 * stride;
        let target_end = start + count as u64;

        // Skip lines before `start`.
        while line_idx < start {
            if line_idx == self.complete_lines {
                break;
            }
            match self.read_one_line_at(file_pos)? {
                Some((consumed, _text, had_nl)) => {
                    file_pos += consumed;
                    if had_nl {
                        line_idx += 1;
                    } else {
                        break;
                    }
                }
                None => break,
            }
        }

        while out.len() < count && line_idx < target_end {
            if line_idx == self.complete_lines {
                if !self.remainder.is_empty() {
                    out.push(decode_line(&self.remainder, self.encoding, self.tab_width));
                }
                break;
            }
            match self.read_one_line_at(file_pos)? {
                Some((consumed, text, had_nl)) => {
                    out.push(text);
                    file_pos += consumed;
                    if had_nl {
                        line_idx += 1;
                    } else {
                        break;
                    }
                }
                None => break,
            }
        }
        Ok(())
    }

    /// Read one line starting at `pos`. Returns (bytes consumed including break, text, had_newline).
    fn read_one_line_at(&mut self, pos: u64) -> io::Result<Option<(u64, String, bool)>> {
        if pos >= self.file_size {
            return Ok(None);
        }
        let mut collected = Vec::new();
        let mut cursor = pos;
        let mut buf = [0u8; 8192];
        loop {
            let n = self.read_at(cursor, &mut buf)?;
            if n == 0 {
                if collected.is_empty() {
                    return Ok(None);
                }
                let text = decode_line(&collected, self.encoding, self.tab_width);
                return Ok(Some((cursor - pos, text, false)));
            }
            let chunk = &buf[..n];
            if let Some((off, brk)) = find_line_break(chunk, self.encoding) {
                collected.extend_from_slice(&chunk[..off]);
                if collected.len() > MAX_LINE_BYTES {
                    collected.truncate(MAX_LINE_BYTES);
                }
                let text = decode_line(&collected, self.encoding, self.tab_width);
                let consumed = (cursor - pos) + off as u64 + brk as u64;
                return Ok(Some((consumed, text, true)));
            }
            collected.extend_from_slice(chunk);
            cursor += n as u64;
            if collected.len() >= MAX_LINE_BYTES {
                collected.truncate(MAX_LINE_BYTES);
                let text = decode_line(&collected, self.encoding, self.tab_width);
                return Ok(Some((cursor - pos, text, false)));
            }
        }
    }

    fn detect_and_reset(&mut self) -> io::Result<()> {
        self.file_size = self.file.metadata()?.len();
        let (encoding, bom_len) = detect_encoding(&mut self.file, self.file_size)?;
        self.encoding = encoding;
        self.bom_len = bom_len;
        self.reset_index_keep_encoding()
    }

    fn reset_index_keep_encoding(&mut self) -> io::Result<()> {
        self.file_size = self.file.metadata()?.len();
        self.checkpoints = vec![self.bom_len];
        self.complete_lines = 0;
        self.remainder_start = self.bom_len;
        self.remainder.clear();
        self.index_complete = self.file_size <= self.bom_len;
        self.preview.clear();
        self.line_cache = None;
        Ok(())
    }

    fn index_chunk(&mut self, max_bytes: usize) -> io::Result<()> {
        if self.index_complete {
            // Still consume newly appended bytes.
        }
        let mut processed = 0usize;
        loop {
            let have = self.remainder_start + self.remainder.len() as u64;
            if have >= self.file_size {
                self.index_complete = true;
                break;
            }
            if processed >= max_bytes {
                break;
            }
            let want = (self.file_size - have).min(64 * 1024) as usize;
            let mut buf = vec![0u8; want];
            let n = self.read_at(have, &mut buf)?;
            if n == 0 {
                self.index_complete = true;
                break;
            }
            processed += n;
            buf.truncate(n);
            self.consume_bytes(&buf);
        }
        if self.remainder_start + self.remainder.len() as u64 >= self.file_size {
            self.index_complete = true;
        }
        Ok(())
    }

    fn consume_bytes(&mut self, chunk: &[u8]) {
        let mut buf = Vec::with_capacity(self.remainder.len() + chunk.len());
        buf.extend_from_slice(&self.remainder);
        buf.extend_from_slice(chunk);
        let base = self.remainder_start;
        let mut pos = 0usize;
        while pos < buf.len() {
            match find_line_break(&buf[pos..], self.encoding) {
                Some((off, brk)) => {
                    self.complete_lines += 1;
                    pos += off + brk;
                    if self.complete_lines % STRIDE as u64 == 0 {
                        self.checkpoints.push(base + pos as u64);
                    }
                }
                None => break,
            }
        }
        self.remainder = buf[pos..].to_vec();
        self.remainder_start = base + pos as u64;
    }

    fn refresh_preview(&mut self) -> io::Result<()> {
        if self.file_size <= self.bom_len {
            self.preview.clear();
            return Ok(());
        }
        let start = self
            .file_size
            .saturating_sub(PREVIEW_BYTES)
            .max(self.bom_len);
        let len = (self.file_size - start) as usize;
        let mut bytes = vec![0u8; len];
        let n = self.read_at(start, &mut bytes)?;
        bytes.truncate(n);
        let slice = if start > self.bom_len {
            match find_line_break(&bytes, self.encoding) {
                Some((off, brk)) => &bytes[off + brk..],
                None => &bytes[..],
            }
        } else {
            &bytes[..]
        };
        self.preview = parse_all_lines(slice, self.encoding, self.tab_width);
        Ok(())
    }

    fn read_at(&mut self, pos: u64, buf: &mut [u8]) -> io::Result<usize> {
        self.file.seek(SeekFrom::Start(pos))?;
        self.file.read(buf)
    }
}

fn open_shared(path: &Path) -> io::Result<File> {
    let mut opts = OpenOptions::new();
    opts.read(true);
    #[cfg(windows)]
    {
        opts.share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE);
    }
    opts.open(path)
}

fn detect_encoding(file: &mut File, size: u64) -> io::Result<(EncodingKind, u64)> {
    if size == 0 {
        return Ok((EncodingKind::Utf8, 0));
    }
    let mut header = [0u8; 4];
    let n = file.seek(SeekFrom::Start(0)).and_then(|_| file.read(&mut header))?;
    if n >= 3 && header[..3] == [0xEF, 0xBB, 0xBF] {
        return Ok((EncodingKind::Utf8, 3));
    }
    if n >= 2 && header[..2] == [0xFF, 0xFE] {
        return Ok((EncodingKind::Utf16Le, 2));
    }
    if n >= 2 && header[..2] == [0xFE, 0xFF] {
        return Ok((EncodingKind::Utf16Be, 2));
    }
    Ok((EncodingKind::Utf8, 0))
}

fn bom_len_for(encoding: EncodingKind, detected: u64) -> u64 {
    match encoding {
        EncodingKind::Utf8 => {
            if detected == 3 {
                3
            } else {
                0
            }
        }
        EncodingKind::Utf16Le | EncodingKind::Utf16Be => {
            if detected == 2 {
                2
            } else {
                0
            }
        }
        EncodingKind::Windows1252 => 0,
    }
}

/// Returns `(offset_of_break, break_len)` relative to `bytes`.
fn find_line_break(bytes: &[u8], encoding: EncodingKind) -> Option<(usize, usize)> {
    match encoding {
        EncodingKind::Utf16Le => find_utf16_break(bytes, true),
        EncodingKind::Utf16Be => find_utf16_break(bytes, false),
        EncodingKind::Utf8 | EncodingKind::Windows1252 => find_ascii_break(bytes),
    }
}

fn find_ascii_break(bytes: &[u8]) -> Option<(usize, usize)> {
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\n' => return Some((i, 1)),
            b'\r' => {
                let len = if i + 1 < bytes.len() && bytes[i + 1] == b'\n' {
                    2
                } else {
                    1
                };
                return Some((i, len));
            }
            _ => i += 1,
        }
    }
    None
}

fn find_utf16_break(bytes: &[u8], le: bool) -> Option<(usize, usize)> {
    let mut i = 0;
    while i + 1 < bytes.len() {
        let c = if le {
            u16::from_le_bytes([bytes[i], bytes[i + 1]])
        } else {
            u16::from_be_bytes([bytes[i], bytes[i + 1]])
        };
        if c == u16::from(b'\n') {
            return Some((i, 2));
        }
        if c == u16::from(b'\r') {
            if i + 3 < bytes.len() {
                let n = if le {
                    u16::from_le_bytes([bytes[i + 2], bytes[i + 3]])
                } else {
                    u16::from_be_bytes([bytes[i + 2], bytes[i + 3]])
                };
                if n == u16::from(b'\n') {
                    return Some((i, 4));
                }
            }
            return Some((i, 2));
        }
        i += 2;
    }
    None
}

fn parse_all_lines(bytes: &[u8], encoding: EncodingKind, tab_width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut pos = 0;
    while pos < bytes.len() {
        match find_line_break(&bytes[pos..], encoding) {
            Some((off, brk)) => {
                lines.push(decode_line(&bytes[pos..pos + off], encoding, tab_width));
                pos += off + brk;
            }
            None => {
                lines.push(decode_line(&bytes[pos..], encoding, tab_width));
                break;
            }
        }
    }
    lines
}

fn decode_line(bytes: &[u8], encoding: EncodingKind, tab_width: usize) -> String {
    let (cow, _, _) = encoding.encoding().decode(bytes);
    expand_tabs_and_strip(&cow, tab_width)
}

fn expand_tabs_and_strip(s: &str, tab_width: usize) -> String {
    let tab_width = tab_width.max(1);
    let mut out = String::with_capacity(s.len());
    let mut col = 0usize;
    let mut chars = 0usize;
    for ch in s.chars() {
        if ch == '\0' || ch == '\r' {
            continue;
        }
        if ch == '\t' {
            let n = tab_width - (col % tab_width);
            for _ in 0..n {
                out.push(' ');
            }
            col += n;
            chars += n;
        } else {
            out.push(ch);
            col += 1;
            chars += 1;
        }
        if chars >= MAX_DISPLAY_CHARS {
            out.push('…');
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn unique_path(tag: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "foxtail-{}-{}-{tag}.log",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        p
    }

    fn write_file(path: &Path, data: &[u8]) {
        let mut f = File::create(path).unwrap();
        f.write_all(data).unwrap();
        f.flush().unwrap();
    }

    fn index_all(t: &mut TailedFile) {
        for _ in 0..64 {
            let _ = t.poll().unwrap();
            if t.is_index_complete() {
                break;
            }
        }
        assert!(t.is_index_complete(), "index did not complete");
    }

    #[test]
    fn lf_and_crlf() {
        let path = unique_path("lf");
        write_file(&path, b"one\ntwo\r\nthree\n");
        let mut t = TailedFile::open(&path, 4).unwrap();
        index_all(&mut t);
        assert_eq!(t.line_count(), 3);
        let lines = t.read_lines(0, 10);
        assert_eq!(lines, vec!["one", "two", "three"]);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn incomplete_last_line() {
        let path = unique_path("inc");
        write_file(&path, b"alpha\nbeta");
        let mut t = TailedFile::open(&path, 4).unwrap();
        index_all(&mut t);
        assert_eq!(t.line_count(), 2);
        assert_eq!(t.read_lines(0, 2), vec!["alpha", "beta"]);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn empty_file() {
        let path = unique_path("empty");
        write_file(&path, b"");
        let mut t = TailedFile::open(&path, 4).unwrap();
        index_all(&mut t);
        assert_eq!(t.line_count(), 0);
        assert!(t.read_lines(0, 5).is_empty());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn append_and_truncate() {
        let path = unique_path("grow");
        write_file(&path, b"a\nb\n");
        let mut t = TailedFile::open(&path, 4).unwrap();
        index_all(&mut t);
        assert_eq!(t.line_count(), 2);

        {
            let mut f = OpenOptions::new().append(true).open(&path).unwrap();
            f.write_all(b"c\nd\n").unwrap();
        }
        let r = t.poll().unwrap();
        assert!(r.grew);
        assert_eq!(t.line_count(), 4);
        assert_eq!(t.read_lines(0, 4), vec!["a", "b", "c", "d"]);

        write_file(&path, b"z\n");
        let r = t.poll().unwrap();
        assert!(r.truncated);
        index_all(&mut t);
        assert_eq!(t.read_lines(0, 5), vec!["z"]);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn utf8_bom() {
        let path = unique_path("bom");
        let mut data = vec![0xEF, 0xBB, 0xBF];
        data.extend_from_slice(b"hello\nworld");
        write_file(&path, &data);
        let mut t = TailedFile::open(&path, 4).unwrap();
        index_all(&mut t);
        assert_eq!(t.encoding(), EncodingKind::Utf8);
        assert_eq!(t.read_lines(0, 2), vec!["hello", "world"]);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn many_lines_stride() {
        let path = unique_path("many");
        let mut body = String::new();
        for i in 0..200 {
            body.push_str(&format!("line-{i}\n"));
        }
        write_file(&path, body.as_bytes());
        let mut t = TailedFile::open(&path, 4).unwrap();
        index_all(&mut t);
        assert_eq!(t.line_count(), 200);
        assert_eq!(t.read_lines(0, 1)[0], "line-0");
        assert_eq!(t.read_lines(31, 1)[0], "line-31");
        assert_eq!(t.read_lines(32, 1)[0], "line-32");
        assert_eq!(t.read_lines(199, 1)[0], "line-199");
        let mid = t.read_lines(100, 5);
        assert_eq!(mid[0], "line-100");
        assert_eq!(mid[4], "line-104");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn tabs_expand() {
        let path = unique_path("tab");
        write_file(&path, b"a\tb\n");
        let mut t = TailedFile::open(&path, 4).unwrap();
        index_all(&mut t);
        assert_eq!(t.read_lines(0, 1)[0], "a   b");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn find_ascii_cr_lf() {
        assert_eq!(find_ascii_break(b"abc\n"), Some((3, 1)));
        assert_eq!(find_ascii_break(b"abc\r\n"), Some((3, 2)));
        assert_eq!(find_ascii_break(b"abc\rdef"), Some((3, 1)));
        assert_eq!(find_ascii_break(b"abc"), None);
    }
}
