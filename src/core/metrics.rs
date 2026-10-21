//! Token counting and output metrics.

use rayon::prelude::*;

use crate::cli::TokenEncoding;

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
    let bytes = text.as_bytes();
    let mut start = 0usize;
    while start < text.len() {
        let mut end = (start + size).min(text.len());
        if end < text.len() {
            // Prefer a line boundary.
            if let Some(pos) = text[start..end].rfind('\n') {
                end = start + pos + 1;
            } else if !text.is_char_boundary(end) {
                while end > start && !text.is_char_boundary(end) {
                    end -= 1;
                }
            }
        }
        if end == start {
            end = (start + size).min(text.len());
        }
        chunks.push(&text[start..end]);
        start = end;
    }
    let _ = bytes;
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
pub fn calculate_file_metrics(
    files: &[(String, String)],
    encoding: TokenEncoding,
) -> Vec<FileMetrics> {
    let token_counts = count_tokens_many(
        &files.iter().map(|(_, c)| c.clone()).collect::<Vec<_>>(),
        encoding,
    );
    files
        .iter()
        .zip(token_counts)
        .map(|((path, content), tokens)| FileMetrics {
            path: path.clone(),
            chars: content.chars().count(),
            tokens,
        })
        .collect()
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
