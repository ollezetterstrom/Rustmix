//! Output rendering.

pub mod json;
pub mod markdown;
pub mod plain;
pub mod style_utils;
pub mod writer;
pub mod xml;

use crate::core::git::GitLogCommit;

/// A file that is part of the packed output.
#[derive(Debug, Clone)]
pub struct ProcessedFile {
    /// The display path of the file.
    pub path: String,
    /// The (processed) content of the file.
    pub content: String,
}

/// Everything the formatters need to render the output.
///
/// The processed files are borrowed instead of owned: cloning them would
/// copy the whole repository content a second time.
#[derive(Debug, Clone, Default)]
pub struct RenderContext<'a> {
    pub generation_header: String,
    pub summary_purpose: String,
    pub summary_file_format: String,
    pub summary_usage_guidelines: String,
    pub summary_notes: String,
    pub header_text: Option<String>,
    pub instruction: Option<String>,
    pub tree_string: String,
    pub processed_files: &'a [ProcessedFile],
    pub file_summary_enabled: bool,
    pub directory_structure_enabled: bool,
    pub files_enabled: bool,
    pub escape_file_content: bool,
    pub markdown_code_block_delimiter: String,
    pub git_diff_enabled: bool,
    pub git_diff_work_tree: String,
    pub git_diff_staged: String,
    pub git_log_enabled: bool,
    pub git_log_commits: Vec<GitLogCommit>,
}

/// Calculate the markdown code fence delimiter.
///
/// The longest run of backticks anywhere in the content determines the fence
/// length; at least three backticks are always used.
pub fn calculate_markdown_delimiter(contents: &[&str]) -> String {
    let mut max_backticks = 0usize;
    for content in contents {
        let mut run = 0usize;
        for c in content.chars() {
            if c == '`' {
                run += 1;
                if run > max_backticks {
                    max_backticks = run;
                }
            } else {
                run = 0;
            }
        }
    }
    "`".repeat(std::cmp::max(3, max_backticks + 1))
}

/// Render the output in the requested style.
pub fn render(style: crate::cli::OutputStyle, ctx: &RenderContext<'_>) -> String {
    match style {
        crate::cli::OutputStyle::Xml => {
            if ctx.escape_file_content {
                xml::render_parsable(ctx)
            } else {
                xml::render(ctx)
            }
        }
        crate::cli::OutputStyle::Markdown => markdown::render(ctx),
        crate::cli::OutputStyle::Json => json::render(ctx),
        crate::cli::OutputStyle::Plain => plain::render(ctx),
    }
}
