//! Terminal reporting.

use std::collections::HashMap;
use std::path::Path;

use crate::config::Config;
use crate::core::metrics::FileMetrics;
use crate::core::packager::PackResult;

/// Format a number with thousands separators (like `toLocaleString`).
pub fn format_number(value: usize) -> String {
    let s = value.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    let bytes = s.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(*b as char);
    }
    out
}

/// Print the "Top N files by token count" block.
pub fn report_top_files(metrics: &[FileMetrics], top_files_len: usize, total_tokens: usize) {
    if top_files_len == 0 {
        return;
    }
    let mut sorted: Vec<&FileMetrics> = metrics.iter().filter(|m| m.tokens > 0).collect();
    sorted.sort_by(|a, b| b.tokens.cmp(&a.tokens));
    let top: Vec<&FileMetrics> = sorted.into_iter().take(top_files_len).collect();
    if top.is_empty() {
        return;
    }
    println!("📈 Top {} Files by Token Count:", top_files_len);
    let mut separator = "─".repeat(29 + top_files_len.to_string().len());
    separator.push_str("");
    println!("{separator}");
    for (index, file) in top.iter().enumerate() {
        let percentage = if total_tokens > 0 {
            ((file.tokens as f64 / total_tokens as f64) * 100.0 * 10.0).round() / 10.0
        } else {
            0.0
        };
        println!(
            "{}. {:<3} {} ({} tokens, {} chars, {}%)",
            index + 1,
            "",
            file.path,
            format_number(file.tokens),
            format_number(file.chars),
            percentage
        );
    }
    println!();
}

/// Print the security check block.
pub fn report_security_check(result: &PackResult) {
    println!("🔎 Security Check:");
    println!("──────────────────");
    if result.suspicious_files.is_empty() {
        println!("✔ No suspicious files detected.");
    } else {
        println!(
            "{} suspicious file(s) detected and excluded from the output:",
            result.suspicious_files.len()
        );
        for (index, file) in result.suspicious_files.iter().enumerate() {
            println!("{}. {:<3} {}", index + 1, "", file.file_path);
            println!(
                "   - {} security issue(s) detected",
                file.messages.len()
            );
        }
        println!();
        println!("These files have been excluded from the output for security reasons.");
        println!("Please review these files for potential sensitive information.");
    }
}

/// Print the binary files block.
pub fn report_skipped_files(result: &PackResult) {
    let binary: Vec<&crate::core::packager::SkippedFile> = result
        .skipped_files
        .iter()
        .filter(|f| f.reason == crate::core::read::SkipReason::BinaryContent)
        .collect();
    if binary.is_empty() {
        return;
    }
    println!("📄 Binary Files Detected:");
    println!("─────────────────────────");
    if binary.len() == 1 {
        println!("1 file detected as binary by content inspection:");
    } else {
        println!(
            "{} files detected as binary by content inspection:",
            binary.len()
        );
    }
    for (index, file) in binary.iter().enumerate() {
        println!("{}. {}", index + 1, file.path);
    }
    println!();
    println!("These files have been excluded from the output.");
    println!("Please review these files if you expected them to contain text content.");
}

/// Print the pack summary.
pub fn report_summary(cfg: &Config, result: &PackResult, output_display: &str) {
    println!("📊 Pack Summary:");
    println!("────────────────");
    println!(
        "  Total Files: {} files",
        format_number(result.metrics.total_files)
    );
    println!(
        " Total Tokens: {} tokens",
        format_number(result.metrics.total_tokens)
    );
    println!(
        "  Total Chars: {} chars",
        format_number(result.metrics.total_characters)
    );
    println!("       Output: {output_display}");
    let security_message = if !cfg.security_check_enabled {
        "Security check disabled".to_string()
    } else if result.suspicious_files.is_empty() {
        "✔ No suspicious files detected".to_string()
    } else {
        format!(
            "{} suspicious file(s) detected and excluded",
            result.suspicious_files.len()
        )
    };
    println!("     Security: {security_message}");
    if cfg.output.git.include_diffs {
        let message = if result.git_diff_tokens > 0 {
            format!(
                "✔ Git diffs included ({} tokens)",
                format_number(result.git_diff_tokens)
            )
        } else {
            "✖ No git diffs included".to_string()
        };
        println!("    Git Diffs: {message}");
    }
    if cfg.output.git.include_logs {
        let message = if result.git_log_tokens > 0 {
            format!(
                "✔ Git logs included ({} tokens)",
                format_number(result.git_log_tokens)
            )
        } else {
            "✖ No git logs included".to_string()
        };
        println!("     Git Logs: {message}");
    }
}

/// Print the completion message.
pub fn report_completion() {
    println!();
    println!("🎉 All Done!");
    println!("Your repository has been successfully packed.");
    println!();
    println!("💡 Repomix is now available in your browser! Try it at https://repomix.com");
}

/// A node in the token count tree.
#[derive(Debug, Default)]
struct TokenTreeNode {
    children: std::collections::BTreeMap<String, TokenTreeNode>,
    tokens: usize,
    is_file: bool,
}

impl TokenTreeNode {
    fn insert(&mut self, path: &str, tokens: usize) {
        self.tokens += tokens;
        let mut current = self;
        let parts: Vec<&str> = path.split('/').collect();
        for (i, part) in parts.iter().enumerate() {
            let is_last = i + 1 == parts.len();
            let child = current.children.entry((*part).to_string()).or_default();
            child.tokens += tokens;
            if is_last {
                child.is_file = true;
            }
            current = child;
        }
    }
}

/// Print the hierarchical token count tree.
pub fn report_token_count_tree(metrics: &[FileMetrics], threshold: u64) {
    println!("🔢 Token Count Tree:");
    println!("────────────────────");
    if threshold > 0 {
        println!("Showing entries with {threshold}+ tokens:");
    }
    let mut root = TokenTreeNode::default();
    for file in metrics {
        root.insert(&file.path, file.tokens);
    }
    print_tree_node(&root, "", true, threshold);
}

fn print_tree_node(node: &TokenTreeNode, prefix: &str, _is_last: bool, threshold: u64) {
    let entries: Vec<(&String, &TokenTreeNode)> = node
        .children
        .iter()
        .filter(|(_, child)| threshold == 0 || child.tokens >= threshold as usize)
        .collect();
    for (i, (name, child)) in entries.iter().enumerate() {
        let is_last_entry = i + 1 == entries.len();
        let connector = if is_last_entry { "└── " } else { "├── " };
        let suffix = if child.is_file {
            format!(" ({} tokens)", format_number(child.tokens))
        } else {
            format!("/ ({} tokens)", format_number(child.tokens))
        };
        println!("{prefix}{connector}{name}{suffix}");
        let child_prefix = format!(
            "{prefix}{}",
            if is_last_entry { "    " } else { "│   " }
        );
        print_tree_node(child, &child_prefix, is_last_entry, threshold);
    }
}

/// Build a display path relative to the current directory.
pub fn display_path(path: &Path, cwd: &Path) -> String {
    let relative = crate::core::packager::path_relative(cwd, path);
    if relative.is_empty() || relative.starts_with("..") {
        path.to_string_lossy().to_string()
    } else {
        relative
    }
}

/// Group the file metrics by directory for the token tree.
pub fn metrics_by_path(metrics: &[FileMetrics]) -> HashMap<String, usize> {
    metrics
        .iter()
        .map(|m| (m.path.clone(), m.tokens))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_numbers() {
        assert_eq!(format_number(0), "0");
        assert_eq!(format_number(999), "999");
        assert_eq!(format_number(1000), "1,000");
        assert_eq!(format_number(1234567), "1,234,567");
    }
}
