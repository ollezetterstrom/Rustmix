//! Token counting and output metrics.

use rayon::prelude::*;

use crate::cli::TokenEncoding;
use crate::output::ProcessedFile;

/// The tokenizer used for counting.
fn bpe(encoding: TokenEncoding) -> &'static tiktoken_rs::CoreBPE {
    match encoding {
        TokenEncoding::O200kBase => tiktoken_rs::o200k_base_singleton(),
        TokenEncoding::Cl100kBase => tiktoken_rs::cl100k_base_singleton(),
        TokenEncoding::P50kBase => tiktoken_rs::p50k_base_singleton(),
        TokenEncoding::P50kEdit => tiktoken_rs::p50k_edit_singleton(),
        TokenEncoding::R50kBase => tiktoken_rs::r50k_base_singleton(),
    }
}

/// Warm up the tokenizer in the background.
///
/// Building the BPE tables takes ~85 ms and would otherwise happen in the
/// middle of the metrics phase, after all the file work is already done.
/// Starting it at the beginning of a run hides that cost behind the file
/// system I/O.
pub fn warm_up(encoding: TokenEncoding) {
    std::thread::spawn(move || {
        let _ = bpe(encoding);
    });
}

/// Count the tokens in a piece of text.
pub fn count_tokens(text: &str, encoding: TokenEncoding) -> usize {
    bpe(encoding).encode_ordinary(text).len()
}

/// Count the tokens of several strings in parallel.
pub fn count_tokens_many(texts: &[String], encoding: TokenEncoding) -> Vec<usize> {
    let bpe = bpe(encoding);
    texts
        .par_iter()
        .map(|t| bpe.encode_ordinary(t).len())
        .collect()
}

/// Count the tokens of a large text, chunking it to bound memory usage.
pub fn count_tokens_chunked(text: &str, encoding: TokenEncoding) -> usize {
    const TARGET_CHARS_PER_CHUNK: usize = 200_000;
    if text.len() <= 1_000_000 {
        return count_tokens(text, encoding);
    }
    let chunks: Vec<&str> = split_chunks(text, TARGET_CHARS_PER_CHUNK);
    let bpe = bpe(encoding);
    chunks
        .par_iter()
        .map(|c| bpe.encode_ordinary(c).len())
        .sum()
}

/// Split a string into chunks of at most `size` characters, preferring line
/// boundaries.
fn split_chunks(text: &str, size: usize) -> Vec<&str> {
    let mut chunks = Vec::new();
    let mut start = 0usize;
    while start < text.len() {
        let mut end = (start + size).min(text.len());
        if end < text.len() {
            // Move back to a character boundary.
            while end > start && !text.is_char_boundary(end) {
                end -= 1;
            }
            // Prefer a line boundary.
            if let Some(pos) = text[start..end].rfind('\n') {
                end = start + pos + 1;
            }
        }
        if end == start {
            end = (start + size).min(text.len());
            while end > start && !text.is_char_boundary(end) {
                end -= 1;
            }
        }
        chunks.push(&text[start..end]);
        start = end;
    }
    chunks
}

/// Per-file metrics.
#[derive(Debug, Default, Clone)]
pub struct FileMetrics {
    pub path: String,
    pub chars: usize,
    pub tokens: usize,
}

/// Aggregated metrics for the packed output.
#[derive(Debug, Default, Clone)]
pub struct Metrics {
    pub total_files: usize,
    pub total_characters: usize,
    pub total_tokens: usize,
    pub file_metrics: Vec<FileMetrics>,
    pub git_diff_tokens: usize,
    pub git_log_tokens: usize,
}

/// Compute per-file metrics for the processed files.
///
/// The token count of a file is the count of the *whole* file content - the
/// reference implementation counts every file as a whole and never splits it
/// into chunks (only the output itself is chunked, and only when the fast
/// path is not available).
///
/// Identical contents are only counted once. A repository contains a lot of
/// duplicated files (the same `LICENSE`, the same type declarations), and
/// hashing all of them would cost more than the counting it saves. The
/// fingerprint below is `O(1)` per file, and the full contents are only
/// compared when two files have the same length *and* the same first and
/// last eight bytes.
pub fn calculate_file_metrics(files: &[ProcessedFile], encoding: TokenEncoding) -> Vec<FileMetrics> {
    // Group the files by their content.
    let mut groups: Vec<usize> = Vec::with_capacity(files.len());
    let mut file_to_group: Vec<usize> = Vec::with_capacity(files.len());
    let mut buckets: std::collections::HashMap<(usize, u64, u64), Vec<usize>> =
        std::collections::HashMap::with_capacity(files.len());
    for (index, file) in files.iter().enumerate() {
        let key = fingerprint(file.content.as_bytes());
        let bucket = buckets.entry(key).or_default();
        let mut existing = None;
        for &candidate in bucket.iter() {
            if files[groups[candidate]].content == file.content {
                existing = Some(candidate);
                break;
            }
        }
        match existing {
            Some(group) => file_to_group.push(group),
            None => {
                groups.push(index);
                file_to_group.push(groups.len() - 1);
                bucket.push(groups.len() - 1);
            }
        }
    }

    // Count the unique contents. They are handed to the thread pool from the
    // largest to the smallest one: a handful of bundled files dominate the
    // running time, and starting with them keeps every thread busy until the
    // very end.
    let mut order: Vec<usize> = (0..groups.len()).collect();
    order.sort_unstable_by_key(|&group| std::cmp::Reverse(files[groups[group]].content.len()));

    let counted: Vec<(usize, usize, usize)> = order
        .par_iter()
        .with_max_len(1)
        .map(|&group| {
            let content = &files[groups[group]].content;
            (
                group,
                count_tokens(content, encoding),
                utf16_len(content),
            )
        })
        .collect();

    let mut tokens_of = vec![0usize; groups.len()];
    let mut chars_of = vec![0usize; groups.len()];
    for (group, tokens, chars) in counted {
        tokens_of[group] = tokens;
        chars_of[group] = chars;
    }

    files
        .iter()
        .enumerate()
        .map(|(index, file)| {
            let group = file_to_group[index];
            FileMetrics {
                path: file.path.clone(),
                chars: chars_of[group],
                tokens: tokens_of[group],
            }
        })
        .collect()
}

/// A cheap fingerprint of a file's content.
///
/// The length plus the first and the last eight bytes. Two files with the
/// same fingerprint are very likely identical, but the contents are always
/// compared in full before they are treated as equal.
#[inline]
fn fingerprint(bytes: &[u8]) -> (usize, u64, u64) {
    let len = bytes.len();
    if len == 0 {
        return (0, 0, 0);
    }
    let head = if len >= 8 {
        u64::from_le_bytes(bytes[..8].try_into().unwrap())
    } else {
        bytes[0] as u64
    };
    let tail = if len >= 8 {
        u64::from_le_bytes(bytes[len - 8..].try_into().unwrap())
    } else {
        bytes[len - 1] as u64
    };
    (len, head, tail)
}

/// The length of a string in UTF-16 code units.
///
/// The reference implementation is JavaScript, where `String.length` counts
/// UTF-16 code units - so an emoji outside the basic multilingual plane
/// counts as two. Counting the Unicode scalar values instead would report a
/// smaller number for every such character.
///
/// The count is derived from the bytes instead of decoding the text: every
/// byte that is not a continuation byte (`0b10xx_xxxx`) starts a character,
/// and every four byte sequence is a character outside the basic
/// multilingual plane, which needs a surrogate pair.
pub fn utf16_len(text: &str) -> usize {
    let bytes = text.as_bytes();
    let mut count = 0usize;
    for &b in bytes {
        // `(b as i8) >= -0x40` is true for every byte that is not a
        // continuation byte, i.e. for every character start.
        count += ((b as i8) >= -0x40) as usize;
        // A four byte sequence is a character outside the BMP.
        count += (b >= 0xF0) as usize;
    }
    count
}

/// Can the total token count be derived from the per-file counts plus the
/// "wrapper" (everything that is not file content)?
///
/// This mirrors the reference implementation's `canUseFastOutputTokenPath`:
/// the fast path is only available for a single output part, without the
/// parsable style, and for the xml / markdown / plain styles.
pub fn can_use_fast_output_token_path(cfg: &crate::config::Config) -> bool {
    if cfg.output.split_output.is_some() {
        return false;
    }
    if cfg.output.parsable_style {
        return false;
    }
    matches!(
        cfg.output.style,
        crate::cli::OutputStyle::Xml | crate::cli::OutputStyle::Markdown | crate::cli::OutputStyle::Plain
    )
}

/// Extract the parts of `output` that are *not* file content.
///
/// This mirrors the reference implementation's `extractOutputWrapper`: the
/// wrapper is everything between the file contents in the rendered output.
/// Counting the wrapper is orders of magnitude cheaper than counting the
/// whole output again, and it reproduces the reference implementation's
/// token total exactly (tokenizing the whole output would merge tokens
/// across the file boundaries and produce a different number).
///
/// The reference implementation searches the output with `indexOf`, so the
/// *first* occurrence of a file's content at or after the cursor is used -
/// even when that is not the place where the file was rendered. That
/// behaviour is reproduced here on purpose.
///
/// The search itself does not use `str::find`: that builds a two-way
/// searcher for every needle, which costs as much as the whole corpus. The
/// first byte of the content is searched with `memchr` instead and the rest
/// is compared directly.
pub fn extract_output_wrapper(
    output: &str,
    processed_files_in_output_order: &[ProcessedFile],
) -> Option<String> {
    let out_bytes = output.as_bytes();
    let mut wrapper = String::with_capacity(output.len() / 16 + 64);
    let mut cursor = 0usize;

    for file in processed_files_in_output_order {
        if file.content.is_empty() {
            continue;
        }
        let needle = file.content.as_bytes();
        let idx = find_from(out_bytes, cursor, needle)?;
        wrapper.push_str(&output[cursor..idx]);
        cursor = idx + needle.len();
    }
    wrapper.push_str(&output[cursor..]);
    Some(wrapper)
}

/// Find `needle` in `haystack` at or after `from`.
///
/// Returns the index of the first occurrence, or `None` when there is none.
/// This is the equivalent of `haystack[from..].find(needle)`, but it does
/// not build a searcher for the needle.
fn find_from(haystack: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(from);
    }
    let first = needle[0];
    let mut position = from;
    while position < haystack.len() {
        let offset = memchr::memchr(first, &haystack[position..])?;
        let candidate = position + offset;
        if haystack.get(candidate..candidate + needle.len()) == Some(needle) {
            return Some(candidate);
        }
        position = candidate + 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_tokens() {
        let n = count_tokens("hello world", TokenEncoding::O200kBase);
        assert!(n > 0 && n < 10);
    }

    #[test]
    fn counts_tokens_in_parallel() {
        let texts = vec!["hello".to_string(), "world".to_string()];
        let counts = count_tokens_many(&texts, TokenEncoding::O200kBase);
        assert_eq!(counts.len(), 2);
        assert!(counts[0] > 0);
    }

    /// The byte based UTF-16 length has to be exactly the number of UTF-16
    /// code units.
    #[test]
    fn utf16_len_matches_the_javascript_length() {
        let samples = [
            "",
            "hello",
            "åäö",
            "日本語",
            "emoji: 😀😃",
            "mixed åäö 日本語 😀 text",
            "\u{10FFFF}",
            "a\u{10FFFF}b",
            "\u{0}\u{1}\u{7F}",
        ];
        for sample in samples {
            assert_eq!(
                utf16_len(sample),
                sample.encode_utf16().count(),
                "mismatch for {sample:?}"
            );
        }
        // A few random strings as well.
        let mut state = 0x1234_5678_9abc_def0u64;
        for _ in 0..200 {
            let mut text = String::new();
            for _ in 0..40 {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                let c = char::from_u32((state >> 32) as u32 % 0x2_0000);
                if let Some(c) = c {
                    text.push(c);
                }
            }
            assert_eq!(utf16_len(&text), text.encode_utf16().count());
        }
    }

    /// The fast search has to find exactly what `str::find` finds.
    #[test]
    fn find_from_matches_str_find() {
        let haystacks = [
            "",
            "abc",
            "xxabcxxabc",
            "the quick brown fox",
            "aaaa",
            "\u{00e5}\u{00e4}\u{00f6}",
        ];
        let needles = ["", "a", "abc", "xx", "zzz", "\u{00e5}", "fox"];
        for haystack in haystacks {
            for needle in needles {
                for from in haystack.char_indices().map(|(i, _)| i).chain([haystack.len()]) {
                    let expected = haystack[from..]
                        .find(needle)
                        .map(|index| index + from);
                    let actual = find_from(haystack.as_bytes(), from, needle.as_bytes());
                    assert_eq!(actual, expected, "{haystack:?} {needle:?} {from}");
                }
            }
        }
    }

    /// The wrapper has to be the same as the one the reference implementation
    /// computes with `indexOf`.
    #[test]
    fn extracts_the_same_wrapper_as_the_reference() {
        let files = vec![
            ProcessedFile {
                path: "a.txt".to_string(),
                content: "hello".to_string(),
            },
            ProcessedFile {
                path: "b.txt".to_string(),
                content: "world".to_string(),
            },
            ProcessedFile {
                path: "c.txt".to_string(),
                content: String::new(),
            },
        ];
        let output = "<files>\n<file path=\"a.txt\">hello</file>\n<file path=\"b.txt\">world</file>\n</files>";

        // The reference implementation.
        let mut expected = String::new();
        let mut cursor = 0usize;
        for file in &files {
            if file.content.is_empty() {
                continue;
            }
            let idx = output[cursor..].find(file.content.as_str()).unwrap() + cursor;
            expected.push_str(&output[cursor..idx]);
            cursor = idx + file.content.len();
        }
        expected.push_str(&output[cursor..]);

        assert_eq!(
            extract_output_wrapper(output, &files).as_deref(),
            Some(expected.as_str())
        );
    }

    /// Duplicated files share their token count, but every file still gets
    /// its own metrics.
    #[test]
    fn counts_duplicated_files_once() {
        let files = vec![
            ProcessedFile {
                path: "a.txt".to_string(),
                content: "hello world".to_string(),
            },
            ProcessedFile {
                path: "b.txt".to_string(),
                content: "hello world".to_string(),
            },
            ProcessedFile {
                path: "c.txt".to_string(),
                content: "a completely different piece of text".to_string(),
            },
        ];
        let metrics = calculate_file_metrics(&files, TokenEncoding::O200kBase);
        assert_eq!(metrics.len(), 3);
        assert_eq!(metrics[0].tokens, metrics[1].tokens);
        assert_eq!(metrics[0].chars, metrics[1].chars);
        assert_eq!(metrics[0].chars, "hello world".encode_utf16().count());
        assert_ne!(metrics[0].tokens, metrics[2].tokens);
        assert_eq!(
            metrics[2].chars,
            "a completely different piece of text".encode_utf16().count()
        );
    }
}
