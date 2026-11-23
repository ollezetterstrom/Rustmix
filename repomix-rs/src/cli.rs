//! Command line interface.
//!
//! The flag set mirrors the official Repomix CLI so that existing scripts and
//! CI pipelines keep working unchanged.

use clap::{Parser, ValueEnum};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OutputStyle {
    Compact,
    Xml,
    Markdown,
    Json,
    Plain,
}

impl OutputStyle {
    pub fn as_str(self) -> &'static str {
        match self {
            OutputStyle::Compact => "compact",
            OutputStyle::Xml => "xml",
            OutputStyle::Markdown => "markdown",
            OutputStyle::Json => "json",
            OutputStyle::Plain => "plain",
        }
    }

    /// Default output filename for this style.
    pub fn default_file_name(self) -> &'static str {
        match self {
            OutputStyle::Compact => "repomix-output.txt",
            OutputStyle::Xml => "repomix-output.xml",
            OutputStyle::Markdown => "repomix-output.md",
            OutputStyle::Json => "repomix-output.json",
            OutputStyle::Plain => "repomix-output.txt",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum FilePathStyle {
    /// Paths are relative to the target directory.
    TargetRelative,
    /// Paths are relative to the current working directory.
    CwdRelative,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum TokenEncoding {
    #[value(name = "o200k_base")]
    O200kBase,
    #[value(name = "cl100k_base")]
    Cl100kBase,
    #[value(name = "p50k_base")]
    P50kBase,
    #[value(name = "p50k_edit")]
    P50kEdit,
    #[value(name = "r50k_base")]
    R50kBase,
}

impl TokenEncoding {
    pub fn as_str(self) -> &'static str {
        match self {
            TokenEncoding::O200kBase => "o200k_base",
            TokenEncoding::Cl100kBase => "cl100k_base",
            TokenEncoding::P50kBase => "p50k_base",
            TokenEncoding::P50kEdit => "p50k_edit",
            TokenEncoding::R50kBase => "r50k_base",
        }
    }
}

/// Repomix - Pack your repository into a single AI-friendly file
#[derive(Debug, Parser)]
#[command(
    name = "repomix",
    version,
    about = "Repomix - Pack your repository into a single AI-friendly file",
    disable_help_subcommand = true
)]
pub struct Cli {
    /// list of directories to process
    #[arg(value_name = "directories", default_value = ".")]
    pub directories: Vec<String>,

    // ------------------------------------------------------------------
    // Basic Options
    // ------------------------------------------------------------------
    /// Enable detailed debug logging
    #[arg(long, conflicts_with = "quiet")]
    pub verbose: bool,

    /// Suppress all console output except errors
    #[arg(long, conflicts_with = "verbose")]
    pub quiet: bool,

    /// Write packed output directly to stdout instead of a file
    #[arg(long)]
    pub stdout: bool,

    /// Read file paths from stdin, one per line
    #[arg(long)]
    pub stdin: bool,

    /// Copy the generated output to system clipboard after processing
    #[arg(long)]
    pub copy: bool,

    /// Show file tree with token counts
    #[arg(
        long,
        value_name = "threshold",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "0"
    )]
    pub token_count_tree: Option<u64>,

    /// Number of largest files to show in summary
    #[arg(long = "top-files-len", value_name = "number", default_value_t = 5)]
    pub top_files_len: usize,

    // ------------------------------------------------------------------
    // Repomix Output Options
    // ------------------------------------------------------------------
    /// Output file path (use "-" for stdout)
    #[arg(short = 'o', long, value_name = "file")]
    pub output: Option<String>,

    /// Output format: compact, xml, markdown, json, or plain
    #[arg(long, value_name = "type", value_enum, default_value = "compact")]
    pub style: OutputStyle,

    /// How file paths are shown in output
    #[arg(long, value_name = "style", value_enum, default_value = "target-relative")]
    pub output_file_path_style: FilePathStyle,

    /// Escape special characters to ensure valid XML/Markdown
    #[arg(long)]
    pub parsable_style: bool,

    /// Extract essential code structure using Tree-sitter parsing
    #[arg(long)]
    pub compress: bool,

    /// Keep top N most-frequently-changed files in full while Tree-sitter compressing the rest
    #[arg(
        long,
        value_name = "top_n",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "10"
    )]
    pub smart_compress: Option<usize>,

    /// Convert 4-space leading indentation to tabs to reduce tokens
    #[arg(long)]
    pub tab_indent: bool,

    /// Prefix each line with its line number in the output
    #[arg(long)]
    pub output_show_line_numbers: bool,

    /// Omit the file summary section from output
    #[arg(long)]
    pub no_file_summary: bool,

    /// Omit the directory tree visualization from output
    #[arg(long)]
    pub no_directory_structure: bool,

    /// Generate metadata only without file contents
    #[arg(long)]
    pub no_files: bool,

    /// Strip all code comments before packing
    #[arg(long)]
    pub remove_comments: bool,

    /// Remove blank lines from all files
    #[arg(long)]
    pub remove_empty_lines: bool,

    /// Truncate long base64 data strings to reduce output size
    #[arg(long)]
    pub truncate_base64: bool,

    /// Custom text to include at the beginning of the output
    #[arg(long, value_name = "text")]
    pub header_text: Option<String>,

    /// Path to file containing custom instructions to include in output
    #[arg(long, value_name = "path")]
    pub instruction_file_path: Option<String>,

    /// Split output into multiple numbered files (e.g. 500kb, 2mb, 2.5mb)
    #[arg(long, value_name = "size", value_parser = crate::util::size::parse_human_size)]
    pub split_output: Option<u64>,

    /// Include folders with no files in directory structure
    #[arg(long)]
    pub include_empty_directories: bool,

    /// Show entire repository tree in the Directory Structure section
    #[arg(long)]
    pub include_full_directory_structure: bool,

    /// Don't sort files by git change frequency
    #[arg(long)]
    pub no_git_sort_by_changes: bool,

    /// Add git diff section showing working tree and staged changes
    #[arg(long)]
    pub include_diffs: bool,

    /// Add git commit history with messages and changed files
    #[arg(long, value_name = "count", num_args = 0..=1, require_equals = true, default_missing_value = "50")]
    pub include_logs: Option<u64>,

    /// Number of recent commits to include with --include-logs
    #[arg(long, value_name = "count")]
    pub include_logs_count: Option<u64>,

    // ------------------------------------------------------------------
    // File Selection Options
    // ------------------------------------------------------------------
    /// Include only files matching these glob patterns (comma-separated)
    #[arg(long, value_name = "patterns")]
    pub include: Option<String>,

    /// Additional patterns to exclude (comma-separated)
    #[arg(short = 'i', long, value_name = "patterns")]
    pub ignore: Option<String>,

    /// Don't use .gitignore rules for filtering files
    #[arg(long)]
    pub no_gitignore: bool,

    /// Don't use .ignore rules for filtering files
    #[arg(long)]
    pub no_dot_ignore: bool,

    /// Don't apply built-in ignore patterns
    #[arg(long)]
    pub no_default_patterns: bool,

    // ------------------------------------------------------------------
    // Configuration Options
    // ------------------------------------------------------------------
    /// Use custom config file instead of repomix.config.json
    #[arg(short = 'c', long, value_name = "path")]
    pub config: Option<String>,

    /// Create a new repomix.config.json file with defaults
    #[arg(long)]
    pub init: bool,

    /// With --init, create config in home directory instead of current directory
    #[arg(long)]
    pub global: bool,

    // ------------------------------------------------------------------
    // Security Options
    // ------------------------------------------------------------------
    /// Skip scanning for sensitive data like API keys and passwords
    #[arg(long)]
    pub no_security_check: bool,

    // ------------------------------------------------------------------
    // Token Count Options
    // ------------------------------------------------------------------
    /// Tokenizer model for counting
    #[arg(long, value_name = "encoding", value_enum, default_value = "o200k_base")]
    pub token_count_encoding: TokenEncoding,

    /// Calculate exact token counts using tiktoken instead of fast byte estimate
    #[arg(long)]
    pub exact_tokens: bool,

    /// Fail with a non-zero exit code when the packed output exceeds N tokens
    #[arg(long, value_name = "number")]
    pub token_budget: Option<u64>,
}

/// Rewrite `--flag value` into `--flag=value` for options that accept an
/// optional value.
///
/// Commander (used by the original Repomix) only accepts an optional value
/// when it is attached with `=`. The official help text nevertheless shows
/// `--token-count-tree 100`, so both spellings are accepted here.
pub fn normalize_optional_value_args(args: &[String]) -> Vec<String> {
    const OPTIONAL_VALUE_FLAGS: [&str; 3] = ["--token-count-tree", "--include-logs", "--smart-compress"];
    let mut out = Vec::with_capacity(args.len());
    let mut i = 0usize;
    while i < args.len() {
        let arg = &args[i];
        if OPTIONAL_VALUE_FLAGS.contains(&arg.as_str()) {
            if let Some(next) = args.get(i + 1) {
                if !next.starts_with('-') && next.chars().all(|c| c.is_ascii_digit()) {
                    out.push(format!("{arg}={next}"));
                    i += 2;
                    continue;
                }
            }
        }
        out.push(arg.clone());
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_optional_value_flags() {
        let args: Vec<String> = ["--token-count-tree", "100", "--include-logs", "10", "--smart-compress", "5"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let out = normalize_optional_value_args(&args);
        assert_eq!(out[0], "--token-count-tree=100");
        assert_eq!(out[1], "--include-logs=10");
        assert_eq!(out[2], "--smart-compress=5");
    }

    #[test]
    fn keeps_directories_intact() {
        let args: Vec<String> = ["--token-count-tree", "."].iter().map(|s| s.to_string()).collect();
        let out = normalize_optional_value_args(&args);
        assert_eq!(out, vec!["--token-count-tree", "."]);
    }
}
