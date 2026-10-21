//! Binary-file detection.
//!
//! Mirrors Repomix's behaviour:
//!
//! 1. The file extension is checked against the `binary-extensions` list.
//! 2. The buffer is scanned for a NUL byte — unless it starts with one of the
//!    recognised text BOMs (UTF-8/16/32, GB18030).
//! 3. If the content is not valid UTF-8, a secondary heuristic decides
//!    whether the file is binary or merely encoded in another charset.

use std::collections::HashSet;
use std::sync::OnceLock;

/// Extension list from the `binary-extensions` package (v3.1.0).
const BINARY_EXTENSIONS: &[&str] = &[
    "3dm", "3ds", "3g2", "3gp", "7z", "a", "aac", "adp", "afdesign", "afphoto", "afpub", "ai", "aif",
    "aiff", "alz", "ape", "apk", "appimage", "ar", "arj", "asf", "au", "avi", "bak", "baml", "bh",
    "bin", "bk", "bmp", "btif", "bz2", "bzip2", "cab", "caf", "cgm", "class", "cmx", "cpio", "cr2",
    "cr3", "cur", "dat", "dcm", "deb", "dex", "djvu", "dll", "dmg", "dng", "doc", "docm", "docx",
    "dot", "dotm", "dra", "ds_Store", "dsk", "dts", "dtshd", "dvb", "dwg", "dxf", "ecelp4800",
    "ecelp7470", "ecelp9600", "egg", "eol", "eot", "epub", "exe", "f4v", "fbs", "fh", "fla", "flac",
    "flatpak", "fli", "flv", "fpx", "fst", "fvt", "g3", "gh", "gif", "graffle", "gz", "gzip", "h261",
    "h263", "h264", "icns", "ico", "ief", "img", "ipa", "iso", "jar", "jpeg", "jpg", "jpgv", "jpm",
    "jxr", "key", "ktx", "lha", "lib", "lvp", "lz", "lzh", "lzma", "lzo", "m3u", "m4a", "m4v", "mar",
    "mdi", "mht", "mid", "midi", "mj2", "mka", "mkv", "mmr", "mng", "mobi", "mov", "movie", "mp3",
    "mp4", "mp4a", "mpeg", "mpg", "mpga", "mxu", "nef", "npx", "numbers", "nupkg", "o", "odp", "ods",
    "odt", "oga", "ogg", "ogv", "otf", "ott", "pages", "pbm", "pcx", "pdb", "pdf", "pea", "pgm",
    "pic", "png", "pnm", "pot", "potm", "potx", "ppa", "ppam", "ppm", "pps", "ppsm", "ppsx", "ppt",
    "pptm", "pptx", "psd", "pya", "pyc", "pyo", "pyv", "qt", "rar", "ras", "raw", "resources", "rgb",
    "rip", "rlc", "rmf", "rmvb", "rpm", "rtf", "rz", "s3m", "s7z", "scpt", "sgi", "shar", "snap",
    "sil", "sketch", "slk", "smv", "snk", "so", "stl", "suo", "sub", "swf", "tar", "tbz", "tbz2",
    "tga", "tgz", "thmx", "tif", "tiff", "tlz", "ttc", "ttf", "txz", "udf", "uvh", "uvi", "uvm",
    "uvp", "uvs", "uvu", "viv", "vob", "war", "wav", "wax", "wbmp", "wdp", "weba", "webm", "webp",
    "whl", "wim", "wm", "wma", "wmv", "wmx", "woff", "woff2", "wrm", "wvx", "xbm", "xif", "xla",
    "xlam", "xls", "xlsb", "xlsm", "xlsx", "xlt", "xltm", "xltx", "xm", "xmind", "xpi", "xpm",
    "xwd", "xz", "z", "zip", "zipx",
];

fn binary_extension_set() -> &'static HashSet<&'static str> {
    static SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
    SET.get_or_init(|| BINARY_EXTENSIONS.iter().copied().collect())
}

/// Does the path carry a known-binary extension?
pub fn is_binary_path(path: &str) -> bool {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let Some(dot) = name.rfind('.') else {
        return false;
    };
    if dot == 0 {
        // Dotfiles such as `.gitignore` have no extension.
        return false;
    }
    let ext = &name[dot + 1..];
    binary_extension_set().contains(&ext.to_ascii_lowercase().as_str())
}

/// Recognised text byte-order marks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bom {
    Utf8,
    Utf32Be,
    Utf32Le,
    Gb18030,
    Utf16Be,
    Utf16Le,
}

/// Detect a text BOM at the start of the buffer.
pub fn detect_text_bom(buf: &[u8]) -> Option<Bom> {
    if buf.starts_with(&[0xEF, 0xBB, 0xBF]) {
        Some(Bom::Utf8)
    } else if buf.starts_with(&[0x00, 0x00, 0xFE, 0xFF]) {
        Some(Bom::Utf32Be)
    } else if buf.starts_with(&[0xFF, 0xFE, 0x00, 0x00]) {
        Some(Bom::Utf32Le)
    } else if buf.starts_with(&[0x84, 0x31, 0x95, 0x33]) {
        Some(Bom::Gb18030)
    } else if buf.starts_with(&[0xFE, 0xFF]) {
        Some(Bom::Utf16Be)
    } else if buf.starts_with(&[0xFF, 0xFE]) {
        Some(Bom::Utf16Le)
    } else {
        None
    }
}

/// The outcome of inspecting a file's bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryVerdict {
    /// Definitely a text file.
    Text,
    /// Definitely binary.
    Binary,
    /// Not valid UTF-8 but not binary either — needs charset decoding.
    NeedsDecoding,
}

/// Decide whether a buffer holds binary content.
///
/// `null_byte_probe` scans the whole buffer for a NUL byte, which is what
/// Repomix does. The SIMD-friendly `memchr` under the hood makes this cheap.
pub fn inspect(buf: &[u8]) -> BinaryVerdict {
    if buf.is_empty() {
        return BinaryVerdict::Text;
    }
    match detect_text_bom(buf) {
        // A UTF-8 BOM is valid UTF-8; the caller strips it.
        Some(Bom::Utf8) => return BinaryVerdict::Text,
        // Other text BOMs exempt the file from the NUL byte probe.
        Some(_) => return BinaryVerdict::NeedsDecoding,
        None => {}
    }
    if buf.contains(&0u8) {
        return BinaryVerdict::Binary;
    }
    if simdutf8::basic::from_utf8(buf).is_ok() {
        return BinaryVerdict::Text;
    }
    if is_binary_content(buf) {
        BinaryVerdict::Binary
    } else {
        BinaryVerdict::NeedsDecoding
    }
}

/// Secondary heuristic used when the buffer is not valid UTF-8.
///
/// This mirrors the `isbinaryfile` package: look for a NUL byte in the first
/// 512 bytes, count "suspicious" bytes, and treat the file as binary when
/// more than 10% of the scanned bytes are suspicious.
fn is_binary_content(buf: &[u8]) -> bool {
    const MAX_BYTES: usize = 512;
    const UTF8_BOUNDARY_RESERVE: usize = 3;
    let scan_len = buf.len().min(MAX_BYTES + UTF8_BOUNDARY_RESERVE);
    let scan = &buf[..scan_len];

    if scan.starts_with(b"%PDF-") {
        return true;
    }
    if detect_text_bom(scan).is_some() {
        return false;
    }
    if scan.iter().take(MAX_BYTES).any(|b| *b == 0) {
        return true;
    }

    let mut suspicious = 0usize;
    let mut i = 0usize;
    let mut iter = scan.iter();
    while let Some(&byte) = iter.next() {
        if byte < 0x80 {
            i += 1;
            continue;
        }
        // Multi-byte UTF-8 sequence?
        let len = match byte {
            0xC2..=0xDF => 2,
            0xE0..=0xEF => 3,
            0xF0..=0xF4 => 4,
            _ => 0,
        };
        if len == 0 {
            suspicious += 1;
        } else {
            let mut ok = true;
            for k in 1..len {
                match iter.next() {
                    Some(&b) if (0x80..=0xBF).contains(&b) => {}
                    _ => {
                        ok = false;
                        break;
                    }
                }
                let _ = k;
            }
            if !ok {
                suspicious += 1;
            }
        }
        i += 1;
        if suspicious > 1 && i >= 32 {
            return true;
        }
    }

    let scan_bytes = scan.len().max(1);
    (suspicious * 100) / scan_bytes > 10
}

/// Strip a leading UTF-8 BOM from decoded text.
pub fn strip_utf8_bom(text: &str) -> &str {
    text.strip_prefix('\u{FEFF}').unwrap_or(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_binary_extensions() {
        assert!(is_binary_path("a/b/image.png"));
        assert!(is_binary_path("a/b/archive.zip"));
        assert!(!is_binary_path("a/b/main.rs"));
        assert!(!is_binary_path("a/b/.gitignore"));
        assert!(!is_binary_path("Makefile"));
    }

    #[test]
    fn detects_boms() {
        assert_eq!(detect_text_bom(&[0xEF, 0xBB, 0xBF, 0x61]), Some(Bom::Utf8));
        assert_eq!(detect_text_bom(&[0xFF, 0xFE, 0x41, 0x00]), Some(Bom::Utf16Le));
        assert_eq!(detect_text_bom(b"hello"), None);
    }

    #[test]
    fn null_bytes_mean_binary() {
        assert_eq!(inspect(b"hello\0world"), BinaryVerdict::Binary);
        assert_eq!(inspect(b"hello world"), BinaryVerdict::Text);
    }

    #[test]
    fn bom_exempts_from_null_probe() {
        let mut buf = vec![0xFF, 0xFE, 0x00, 0x00];
        buf.extend_from_slice(b"text");
        assert_eq!(inspect(&buf), BinaryVerdict::NeedsDecoding);
    }
}
