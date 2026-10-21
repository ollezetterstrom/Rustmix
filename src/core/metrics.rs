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

/// Count the tokens of several strings in parallel without cloning them.
pub fn count_tokens_strs(texts: &[&str], encoding: TokenEncoding) -> Vec<usize> {
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
pub fn calculate_file_metrics(files: &[ProcessedFile], encoding: TokenEncoding) -> Vec<FileMetrics> {
    let contents: Vec<&str> = files.iter().map(|f| f.content.as_str()).collect();
    let token_counts = count_tokens_strs(&contents, encoding);
    files
        .iter()
        .zip(token_counts)
        .map(|(file, tokens)| FileMetrics {
            path: file.path.clone(),
            chars: utf16_len(&file.content),
            tokens,
        })
        .collect()
}

/// The length of a string in UTF-16 code units.
///
/// The reference implementation is JavaScript, where `String.length` counts
/// UTF-16 code units - so an emoji outside the basic multilingual plane
/// counts as two. Counting the Unicode scalar values instead would report a
/// smaller number for every such character.
pub fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
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
pub fn extract_output_wrapper(
    output: &str,
    processed_files_in_output_order: &[ProcessedFile],
) -> Option<String> {
    let mut wrapper = String::new();
    let mut cursor = 0usize;

    for file in processed_files_in_output_order {
        if file.content.is_empty() {
            continue;
        }
        let idx = output[cursor..].find(file.content.as_str())? + cursor;
        wrapper.push_str(&output[cursor..idx]);
        cursor = idx + file.content.len();
    }
    wrapper.push_str(&output[cursor..]);
    Some(wrapper)
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
}
