//! Configuration model and loading.
//!
//! Precedence follows the original implementation: built-in defaults, then
//! the configuration file, then command line flags.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::cli::{Cli, FilePathStyle, OutputStyle, TokenEncoding};

/// Default maximum file size: 50 MiB.
pub const DEFAULT_MAX_FILE_SIZE: u64 = 50 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct OutputConfig {
    pub file_path: String,
    pub style: OutputStyle,
    pub file_path_style: FilePathStyle,
    pub parsable_style: bool,
    pub header_text: Option<String>,
    pub instruction_file_path: Option<String>,
    pub file_summary: bool,
    pub directory_structure: bool,
    pub files: bool,
    pub remove_comments: bool,
    pub remove_empty_lines: bool,
    pub compress: bool,
    pub top_files_length: usize,
    pub show_line_numbers: bool,
    pub truncate_base64: bool,
    pub copy_to_clipboard: bool,
    pub include_empty_directories: Option<bool>,
    pub include_full_directory_structure: bool,
    pub split_output: Option<u64>,
    pub token_count_tree: Option<u64>,
    pub token_budget: Option<u64>,
    pub stdout: bool,
    pub git: GitConfig,
}

#[derive(Debug, Clone)]
pub struct GitConfig {
    pub sort_by_changes: bool,
    pub sort_by_changes_max_commits: usize,
    pub include_diffs: bool,
    pub include_logs: bool,
    pub include_logs_count: usize,
}

impl Default for GitConfig {
    fn default() -> Self {
        Self {
            sort_by_changes: true,
            sort_by_changes_max_commits: 100,
            include_diffs: false,
            include_logs: false,
            include_logs_count: 50,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub cwd: PathBuf,
    pub max_file_size: u64,
    pub output: OutputConfig,
    pub include: Vec<String>,
    pub ignore: IgnoreConfig,
    pub security_check_enabled: bool,
    pub token_encoding: TokenEncoding,
}

#[derive(Debug, Clone)]
pub struct IgnoreConfig {
    pub use_gitignore: bool,
    pub use_dot_ignore: bool,
    pub use_default_patterns: bool,
    pub custom_patterns: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            max_file_size: DEFAULT_MAX_FILE_SIZE,
            output: OutputConfig {
                file_path: "repomix-output.xml".to_string(),
                style: OutputStyle::Xml,
                file_path_style: FilePathStyle::TargetRelative,
                parsable_style: false,
                header_text: None,
                instruction_file_path: None,
                file_summary: true,
                directory_structure: true,
                files: true,
                remove_comments: false,
                remove_empty_lines: false,
                compress: false,
                top_files_length: 5,
                show_line_numbers: false,
                truncate_base64: false,
                copy_to_clipboard: false,
                include_empty_directories: None,
                include_full_directory_structure: false,
                split_output: None,
                token_count_tree: None,
                token_budget: None,
                stdout: false,
                git: GitConfig::default(),
            },
            include: Vec::new(),
            ignore: IgnoreConfig {
                use_gitignore: true,
                use_dot_ignore: true,
                use_default_patterns: true,
                custom_patterns: Vec::new(),
            },
            security_check_enabled: true,
            token_encoding: TokenEncoding::O200kBase,
        }
    }
}

// ---------------------------------------------------------------------------
// Configuration file schema
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FileConfig {
    pub input: Option<FileInputConfig>,
    pub output: Option<FileOutputConfig>,
    pub include: Option<Vec<String>>,
    pub ignore: Option<FileIgnoreConfig>,
    pub security: Option<FileSecurityConfig>,
    pub token_count: Option<FileTokenCountConfig>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FileInputConfig {
    pub max_file_size: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FileOutputConfig {
    pub file_path: Option<String>,
    pub style: Option<String>,
    pub file_path_style: Option<String>,
    pub parsable_style: Option<bool>,
    pub header_text: Option<String>,
    pub instruction_file_path: Option<String>,
    pub file_summary: Option<bool>,
    pub directory_structure: Option<bool>,
    pub files: Option<bool>,
    pub remove_comments: Option<bool>,
    pub remove_empty_lines: Option<bool>,
    pub compress: Option<bool>,
    pub top_files_length: Option<usize>,
    pub show_line_numbers: Option<bool>,
    pub truncate_base64: Option<bool>,
    pub copy_to_clipboard: Option<bool>,
    pub include_empty_directories: Option<bool>,
    pub include_full_directory_structure: Option<bool>,
    pub split_output: Option<u64>,
    pub token_count_tree: Option<serde_json::Value>,
    pub token_budget: Option<u64>,
    pub git: Option<FileGitConfig>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FileGitConfig {
    pub sort_by_changes: Option<bool>,
    pub sort_by_changes_max_commits: Option<usize>,
    pub include_diffs: Option<bool>,
    pub include_logs: Option<bool>,
    pub include_logs_count: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FileIgnoreConfig {
    pub use_gitignore: Option<bool>,
    pub use_dot_ignore: Option<bool>,
    pub use_default_patterns: Option<bool>,
    pub custom_patterns: Option<Vec<String>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FileSecurityConfig {
    pub enable_security_check: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FileTokenCountConfig {
    pub encoding: Option<String>,
}

/// Candidate configuration file names, in discovery order.
pub const CONFIG_FILE_NAMES: &[&str] = &[
    "repomix.config.ts",
    "repomix.config.mts",
    "repomix.config.cts",
    "repomix.config.js",
    "repomix.config.mjs",
    "repomix.config.cjs",
    "repomix.config.json5",
    "repomix.config.jsonc",
    "repomix.config.json",
];

/// Locate the configuration file to use.
pub fn find_config_file(cwd: &Path, explicit: Option<&str>) -> Option<PathBuf> {
    if let Some(p) = explicit {
        let path = if Path::new(p).is_absolute() {
            PathBuf::from(p)
        } else {
            cwd.join(p)
        };
        return if path.exists() { Some(path) } else { None };
    }
    for name in CONFIG_FILE_NAMES {
        let candidate = cwd.join(name);
        // TypeScript / JavaScript config files need a loader we do not have.
        if matches!(
            Path::new(name).extension().and_then(|e| e.to_str()),
            Some("ts") | Some("mts") | Some("cts") | Some("js") | Some("mjs") | Some("cjs")
        ) {
            continue;
        }
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

/// Parse a configuration file (JSON, JSON5 or JSONC).
pub fn parse_config_file(path: &Path) -> Result<FileConfig> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read config file: {}", path.display()))?;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("json")
        .to_ascii_lowercase();
    if ext == "json5" {
        let value: serde_json::Value = json5::from_str(&text)
            .with_context(|| format!("Failed to parse config file: {}", path.display()))?;
        serde_json::from_value(value)
            .with_context(|| format!("Failed to parse config file: {}", path.display()))
    } else {
        // JSONC is a superset of JSON apart from trailing commas; serde_json
        // handles the common case and json5 is used as a fallback.
        match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(value) => serde_json::from_value(value)
                .with_context(|| format!("Failed to parse config file: {}", path.display())),
            Err(_) => {
                let value: serde_json::Value = json5::from_str(&text)
                    .with_context(|| format!("Failed to parse config file: {}", path.display()))?;
                serde_json::from_value(value)
                    .with_context(|| format!("Failed to parse config file: {}", path.display()))
            }
        }
    }
}

/// Split a comma separated pattern list, ignoring commas inside `{}`.
pub fn split_patterns(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut cur = String::new();
    for c in input.chars() {
        match c {
            '{' => {
                depth += 1;
                cur.push(c);
            }
            '}' => {
                depth = depth.saturating_sub(1);
                cur.push(c);
            }
            ',' if depth == 0 => {
                let trimmed = cur.trim().to_string();
                if !trimmed.is_empty() {
                    out.push(trimmed);
                }
                cur.clear();
            }
            _ => cur.push(c),
        }
    }
    let trimmed = cur.trim().to_string();
    if !trimmed.is_empty() {
        out.push(trimmed);
    }
    out
}

impl Config {
    /// Merge configuration file contents and CLI flags into a final config.
    pub fn build(cwd: PathBuf, cli: &Cli, file: Option<&FileConfig>) -> Config {
        let mut cfg = Config { cwd, ..Config::default() };
        let mut file_path_explicit = false;

        if let Some(file) = file {
            if let Some(input) = &file.input {
                if let Some(max) = input.max_file_size {
                    cfg.max_file_size = max;
                }
            }
            if let Some(output) = &file.output {
                if let Some(p) = &output.file_path {
                    cfg.output.file_path = p.clone();
                    file_path_explicit = true;
                }
                if let Some(style) = &output.style {
                    cfg.output.style = parse_style(style);
                }
                if let Some(style) = &output.file_path_style {
                    cfg.output.file_path_style = parse_file_path_style(style);
                }
                if let Some(v) = output.parsable_style {
                    cfg.output.parsable_style = v;
                }
                if let Some(v) = &output.header_text {
                    cfg.output.header_text = Some(v.clone());
                }
                if let Some(v) = &output.instruction_file_path {
                    cfg.output.instruction_file_path = Some(v.clone());
                }
                if let Some(v) = output.file_summary {
                    cfg.output.file_summary = v;
                }
                if let Some(v) = output.directory_structure {
                    cfg.output.directory_structure = v;
                }
                if let Some(v) = output.files {
                    cfg.output.files = v;
                }
                if let Some(v) = output.remove_comments {
                    cfg.output.remove_comments = v;
                }
                if let Some(v) = output.remove_empty_lines {
                    cfg.output.remove_empty_lines = v;
                }
                if let Some(v) = output.compress {
                    cfg.output.compress = v;
                }
                if let Some(v) = output.top_files_length {
                    cfg.output.top_files_length = v;
                }
                if let Some(v) = output.show_line_numbers {
                    cfg.output.show_line_numbers = v;
                }
                if let Some(v) = output.truncate_base64 {
                    cfg.output.truncate_base64 = v;
                }
                if let Some(v) = output.copy_to_clipboard {
                    cfg.output.copy_to_clipboard = v;
                }
                if let Some(v) = output.include_empty_directories {
                    cfg.output.include_empty_directories = Some(v);
                }
                if let Some(v) = output.include_full_directory_structure {
                    cfg.output.include_full_directory_structure = v;
                }
                if let Some(v) = output.split_output {
                    cfg.output.split_output = Some(v);
                }
                if let Some(v) = &output.token_count_tree {
                    cfg.output.token_count_tree = match v {
                        serde_json::Value::Bool(true) => Some(0),
                        serde_json::Value::Bool(false) => None,
                        serde_json::Value::Number(n) => n.as_u64(),
                        serde_json::Value::String(s) => s.parse().ok(),
                        _ => None,
                    };
                }
                if let Some(v) = output.token_budget {
                    cfg.output.token_budget = Some(v);
                }
                if let Some(git) = &output.git {
                    if let Some(v) = git.sort_by_changes {
                        cfg.output.git.sort_by_changes = v;
                    }
                    if let Some(v) = git.sort_by_changes_max_commits {
                        cfg.output.git.sort_by_changes_max_commits = v;
                    }
                    if let Some(v) = git.include_diffs {
                        cfg.output.git.include_diffs = v;
                    }
                    if let Some(v) = git.include_logs {
                        cfg.output.git.include_logs = v;
                    }
                    if let Some(v) = git.include_logs_count {
                        cfg.output.git.include_logs_count = v;
                    }
                }
            }
            if let Some(include) = &file.include {
                cfg.include.extend(include.iter().cloned());
            }
            if let Some(ignore) = &file.ignore {
                if let Some(v) = ignore.use_gitignore {
                    cfg.ignore.use_gitignore = v;
                }
                if let Some(v) = ignore.use_dot_ignore {
                    cfg.ignore.use_dot_ignore = v;
                }
                if let Some(v) = ignore.use_default_patterns {
                    cfg.ignore.use_default_patterns = v;
                }
                if let Some(v) = &ignore.custom_patterns {
                    cfg.ignore.custom_patterns.extend(v.iter().cloned());
                }
            }
            if let Some(security) = &file.security {
                if let Some(v) = security.enable_security_check {
                    cfg.security_check_enabled = v;
                }
            }
            if let Some(token_count) = &file.token_count {
                if let Some(v) = &token_count.encoding {
                    if let Some(enc) = parse_encoding(v) {
                        cfg.token_encoding = enc;
                    }
                }
            }
        }

        // --- CLI overrides -------------------------------------------------
        if let Some(v) = &cli.output {
            if v == "-" {
                cfg.output.stdout = true;
            } else {
                cfg.output.file_path = v.clone();
            }
            file_path_explicit = true;
        }
        cfg.output.style = cli.style;
        cfg.output.file_path_style = cli.output_file_path_style;
        cfg.output.parsable_style |= cli.parsable_style;
        cfg.output.compress |= cli.compress;
        cfg.output.show_line_numbers |= cli.output_show_line_numbers;
        if cli.no_file_summary {
            cfg.output.file_summary = false;
        }
        if cli.no_directory_structure {
            cfg.output.directory_structure = false;
        }
        if cli.no_files {
            cfg.output.files = false;
        }
        cfg.output.remove_comments |= cli.remove_comments;
        cfg.output.remove_empty_lines |= cli.remove_empty_lines;
        cfg.output.truncate_base64 |= cli.truncate_base64;
        if let Some(v) = &cli.header_text {
            cfg.output.header_text = Some(v.clone());
        }
        if let Some(v) = &cli.instruction_file_path {
            cfg.output.instruction_file_path = Some(v.clone());
        }
        if let Some(v) = cli.split_output {
            cfg.output.split_output = Some(v);
        }
        if cli.include_empty_directories {
            cfg.output.include_empty_directories = Some(true);
        }
        cfg.output.include_full_directory_structure |= cli.include_full_directory_structure;
        if cli.no_git_sort_by_changes {
            cfg.output.git.sort_by_changes = false;
        }
        if cli.include_diffs {
            cfg.output.git.include_diffs = true;
        }
        if let Some(count) = cli.include_logs {
            cfg.output.git.include_logs = true;
            cfg.output.git.include_logs_count = count as usize;
        }
        if let Some(count) = cli.include_logs_count {
            cfg.output.git.include_logs = true;
            cfg.output.git.include_logs_count = count as usize;
        }
        if let Some(v) = &cli.include {
            cfg.include.extend(split_patterns(v));
        }
        if let Some(v) = &cli.ignore {
            cfg.ignore.custom_patterns.extend(split_patterns(v));
        }
        if cli.no_gitignore {
            cfg.ignore.use_gitignore = false;
        }
        if cli.no_dot_ignore {
            cfg.ignore.use_dot_ignore = false;
        }
        if cli.no_default_patterns {
            cfg.ignore.use_default_patterns = false;
        }
        if cli.no_security_check {
            cfg.security_check_enabled = false;
        }
        cfg.output.token_count_tree = cli.token_count_tree;
        cfg.output.token_budget = cli.token_budget;
        cfg.output.top_files_length = cli.top_files_len;
        cfg.output.stdout |= cli.stdout;
        cfg.output.copy_to_clipboard |= cli.copy;
        cfg.token_encoding = cli.token_count_encoding;

        // The default output filename follows the selected style unless the
        // user asked for a specific file.
        if !file_path_explicit {
            cfg.output.file_path = cfg.output.style.default_file_name().to_string();
        }

        cfg
    }
}

fn parse_style(style: &str) -> OutputStyle {
    match style.to_ascii_lowercase().as_str() {
        "markdown" | "md" => OutputStyle::Markdown,
        "json" => OutputStyle::Json,
        "plain" | "txt" => OutputStyle::Plain,
        _ => OutputStyle::Xml,
    }
}

fn parse_file_path_style(style: &str) -> FilePathStyle {
    match style.to_ascii_lowercase().as_str() {
        "cwd-relative" => FilePathStyle::CwdRelative,
        _ => FilePathStyle::TargetRelative,
    }
}

fn parse_encoding(encoding: &str) -> Option<TokenEncoding> {
    match encoding.to_ascii_lowercase().as_str() {
        "o200k_base" => Some(TokenEncoding::O200kBase),
        "cl100k_base" => Some(TokenEncoding::Cl100kBase),
        "p50k_base" => Some(TokenEncoding::P50kBase),
        "p50k_edit" => Some(TokenEncoding::P50kEdit),
        "r50k_base" => Some(TokenEncoding::R50kBase),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_patterns_ignoring_braces() {
        assert_eq!(
            split_patterns("src/**/*.rs,**/*.md"),
            vec!["src/**/*.rs", "**/*.md"]
        );
        assert_eq!(split_patterns("a{b,c}d"), vec!["a{b,c}d"]);
    }
}
