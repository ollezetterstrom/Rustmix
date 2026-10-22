//! File reading with binary detection.

use std::path::Path;

use crate::util::binary::{self, BinaryVerdict};

/// Why a file was left out of the packed output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// The extension is a known binary format.
    BinaryExtension,
    /// The file is larger than `input.maxFileSize`.
    SizeLimit,
    /// The content looks binary (NUL bytes / invalid UTF-8 heuristics).
    BinaryContent,
    /// The content could not be decoded as text.
    EncodingError,
}

/// A file that was read from disk.
#[derive(Debug, Clone)]
pub struct RawFile {
    /// Root relative, POSIX separated path.
    pub path: String,
    /// Decoded text content, when the file could be read.
    pub content: Option<String>,
    /// Set when the file was skipped.
    pub skipped: Option<SkipReason>,
}

impl RawFile {
    pub fn is_skipped(&self) -> bool {
        self.skipped.is_some()
    }
}

/// Read a file and decode it as UTF-8 text.
pub fn read_raw_file(abs_path: &Path, rel_path: &str, max_file_size: u64) -> RawFile {
    if binary::is_binary_path(rel_path) {
        return RawFile {
            path: rel_path.to_string(),
            content: None,
            skipped: Some(SkipReason::BinaryExtension),
        };
    }

    let buf = match std::fs::read(abs_path) {
        Ok(b) => b,
        Err(_) => {
            return RawFile {
                path: rel_path.to_string(),
                content: None,
                skipped: Some(SkipReason::EncodingError),
            }
        }
    };

    if buf.len() as u64 > max_file_size {
        return RawFile {
            path: rel_path.to_string(),
            content: None,
            skipped: Some(SkipReason::SizeLimit),
        };
    }

    match binary::inspect(&buf) {
        BinaryVerdict::Binary => RawFile {
            path: rel_path.to_string(),
            content: None,
            skipped: Some(SkipReason::BinaryContent),
        },
        BinaryVerdict::Text => {
            // `inspect` has already checked that the buffer is valid UTF-8
            // (with the SIMD validator), except for a file that starts with
            // a UTF-8 byte order mark - that one is checked here.
            let mut buf = buf;
            if buf.starts_with(&[0xEF, 0xBB, 0xBF]) {
                buf.drain(..3);
                if simdutf8::basic::from_utf8(&buf).is_err() {
                    return RawFile {
                        path: rel_path.to_string(),
                        content: None,
                        skipped: Some(SkipReason::EncodingError),
                    };
                }
            }
            // SAFETY: the buffer was validated above, so it is valid UTF-8.
            let text = unsafe { String::from_utf8_unchecked(buf) };
            RawFile {
                path: rel_path.to_string(),
                content: Some(text),
                skipped: None,
            }
        }
        BinaryVerdict::NeedsDecoding => decode_fallback(&buf, rel_path),
    }
}

/// Decode a buffer that is not valid UTF-8 but does not look binary.
fn decode_fallback(buf: &[u8], rel_path: &str) -> RawFile {
    // UTF-16 with a BOM.
    if buf.starts_with(&[0xFF, 0xFE]) {
        if let Some(text) = decode_utf16(&buf[2..], true) {
            return RawFile {
                path: rel_path.to_string(),
                content: Some(text),
                skipped: None,
            };
        }
    }
    if buf.starts_with(&[0xFE, 0xFF]) {
        if let Some(text) = decode_utf16(&buf[2..], false) {
            return RawFile {
                path: rel_path.to_string(),
                content: Some(text),
                skipped: None,
            };
        }
    }
    // Latin-1 is a lossless byte -> code point mapping; it is a reasonable
    // fallback for single byte encoded files.
    let text: String = buf.iter().map(|b| *b as char).collect();
    if text.contains('\u{FFFD}') {
        return RawFile {
            path: rel_path.to_string(),
            content: None,
            skipped: Some(SkipReason::EncodingError),
        };
    }
    RawFile {
        path: rel_path.to_string(),
        content: Some(text),
        skipped: None,
    }
}

/// Decode UTF-16 (little or big endian) into a Rust string.
fn decode_utf16(bytes: &[u8], little_endian: bool) -> Option<String> {
    if bytes.len() % 2 != 0 {
        return None;
    }
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| {
            if little_endian {
                u16::from_le_bytes([c[0], c[1]])
            } else {
                u16::from_be_bytes([c[0], c[1]])
            }
        })
        .collect();
    String::from_utf16(&units).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_utf8_and_strips_bom() {
        let dir = std::env::temp_dir().join("repomix-rs-test-read");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.txt");
        std::fs::write(&path, b"\xEF\xBB\xBFhello").unwrap();
        let raw = read_raw_file(&path, "a.txt", 1024);
        assert_eq!(raw.content.as_deref(), Some("hello"));
    }

    #[test]
    fn detects_binary_content() {
        let dir = std::env::temp_dir().join("repomix-rs-test-read");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("b.txt");
        std::fs::write(&path, b"abc\0def").unwrap();
        let raw = read_raw_file(&path, "b.txt", 1024);
        assert_eq!(raw.skipped, Some(SkipReason::BinaryContent));
    }
}
