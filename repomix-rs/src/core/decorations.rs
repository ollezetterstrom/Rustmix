//! Generation of the summary sections.

use crate::config::Config;

/// A summary of the selection and processing options.
struct ContentInfo {
    is_entire_codebase: bool,
    include: bool,
    ignore: bool,
    gitignore: bool,
    default_ignore: bool,
    comments_removed: bool,
    empty_lines_removed: bool,
    security_check_enabled: bool,
    show_line_numbers: bool,
    parsable_style: bool,
    compressed: bool,
    truncated_base64: bool,
    git_changes: bool,
}

fn analyze_content(cfg: &Config) -> ContentInfo {
    ContentInfo {
        is_entire_codebase: cfg.include.is_empty() && cfg.ignore.custom_patterns.is_empty(),
        include: !cfg.include.is_empty(),
        ignore: !cfg.ignore.custom_patterns.is_empty(),
        gitignore: cfg.ignore.use_gitignore,
        default_ignore: cfg.ignore.use_default_patterns,
        comments_removed: cfg.output.remove_comments,
        empty_lines_removed: cfg.output.remove_empty_lines,
        security_check_enabled: cfg.security_check_enabled,
        show_line_numbers: cfg.output.show_line_numbers,
        parsable_style: cfg.output.parsable_style,
        compressed: cfg.output.compress,
        truncated_base64: cfg.output.truncate_base64,
        git_changes: cfg.output.git.sort_by_changes,
    }
}

/// The header shown at the top of the output.
pub fn generate_header(cfg: &Config) -> String {
    let info = analyze_content(cfg);
    let description = if info.is_entire_codebase {
        "This file is a merged representation of the entire codebase".to_string()
    } else {
        let mut parts = Vec::new();
        if info.include {
            parts.push("specifically included files");
        }
        if info.ignore {
            parts.push("files not matching ignore patterns");
        }
        format!(
            "This file is a merged representation of a subset of the codebase, containing {}",
            parts.join(" and ")
        )
    };

    let mut processing_notes: Vec<String> = Vec::new();
    if info.comments_removed {
        processing_notes.push("comments have been removed".to_string());
    }
    if info.empty_lines_removed {
        processing_notes.push("empty lines have been removed".to_string());
    }
    if info.show_line_numbers {
        processing_notes.push("line numbers have been added".to_string());
    }
    if info.parsable_style {
        processing_notes.push(format!(
            "content has been formatted for parsing in {} style",
            cfg.output.style.as_str()
        ));
    }
    if info.compressed {
        processing_notes.push(
            "content has been compressed (code blocks are separated by \u{22EE}---- delimiter)".to_string(),
        );
    }
    if !info.security_check_enabled {
        processing_notes.push("security check has been disabled".to_string());
    }

    let processing_info = if processing_notes.is_empty() {
        String::new()
    } else {
        format!(
            "The content has been processed where {}.",
            processing_notes.join(", ")
        )
    };

    format!("{description}, combined into a single document by Rustmix.\n{processing_info}")
        .trim()
        .to_string()
}

/// The purpose section.
pub fn generate_summary_purpose(cfg: &Config) -> String {
    let info = analyze_content(cfg);
    let content_description = if info.is_entire_codebase {
        "the entire repository's contents"
    } else {
        "a subset of the repository's contents that is considered the most important context"
    };
    format!(
        "\nThis file contains a packed representation of {content_description}.\nIt is designed to be easily consumable by AI systems for analysis, code review,\nor other automated processes.\n"
    )
    .trim()
    .to_string()
}

/// The file format section (XML / markdown / plain).
pub fn generate_summary_file_format() -> String {
    "\nThe content is organized as follows:\n1. This summary section\n2. Repository information\n3. Directory structure\n4. Repository files (if enabled)\n"
        .trim()
        .to_string()
}

/// The usage guidelines section.
pub fn generate_summary_usage_guidelines(cfg: &Config, has_instruction: bool) -> String {
    // The reference implementation builds the text from a template with two
    // conditional lines and then trims the result:
    //
    //     `\n<lines>\n${headerText ? A : ''}\n${instruction ? B : ''}\n`.trim()
    //
    // Both conditional lines are always present, so with an instruction but
    // without a header text the empty header line survives the trim and
    // leaves a blank line between the guidelines and the instruction note.
    let header_line = if cfg.output.header_text.is_some() {
        "- Pay special attention to the Repository Description. These contain important context and guidelines specific to this project."
    } else {
        ""
    };
    let instruction_line = if has_instruction {
        "- Pay special attention to the Repository Instruction. These contain important context and guidelines specific to this project."
    } else {
        ""
    };

    let text = format!(
        "\n\
         - This file should be treated as read-only. Any changes should be made to the\n\
         \x20 original repository files, not this packed version.\n\
         - When processing this file, use the file path to distinguish\n\
         \x20 between different files in the repository.\n\
         - Be aware that this file may contain sensitive information. Handle it with\n\
         \x20 the same level of security as you would the original repository.\n\
         {header_line}\n\
         {instruction_line}\n"
    );
    crate::output::style_utils::js_trim(&text).to_string()
}

/// The notes section.
pub fn generate_summary_notes(cfg: &Config) -> String {
    let info = analyze_content(cfg);
    let mut notes = vec![
        "- Some files may have been excluded based on .gitignore rules and Rustmix's configuration".to_string(),
        "- Binary files are not included in this packed representation. Please refer to the Repository Structure section for a complete list of file paths, including binary files".to_string(),
    ];
    if info.include {
        notes.push(format!(
            "- Only files matching these patterns are included: {}",
            cfg.include.join(", ")
        ));
    }
    if info.ignore {
        notes.push(format!(
            "- Files matching these patterns are excluded: {}",
            cfg.ignore.custom_patterns.join(", ")
        ));
    }
    if info.gitignore {
        notes.push("- Files matching patterns in .gitignore are excluded".to_string());
    }
    if info.default_ignore {
        notes.push("- Files matching default ignore patterns are excluded".to_string());
    }
    if info.comments_removed {
        notes.push("- Code comments have been removed from supported file types".to_string());
    }
    if info.empty_lines_removed {
        notes.push("- Empty lines have been removed from all files".to_string());
    }
    if info.show_line_numbers {
        notes.push("- Line numbers have been added to the beginning of each line".to_string());
    }
    if info.parsable_style {
        notes.push(format!(
            "- Content has been formatted for parsing in {} style",
            cfg.output.style.as_str()
        ));
    }
    if info.compressed {
        notes.push(
            "- Content has been compressed - code blocks are separated by \u{22EE}---- delimiter"
                .to_string(),
        );
    }
    if info.truncated_base64 {
        notes.push("- Long base64 data strings (e.g., data:image/png;base64,...) have been truncated to reduce token count".to_string());
    }
    if !info.security_check_enabled {
        notes.push(
            "- Security check has been disabled - content may contain sensitive information"
                .to_string(),
        );
    }
    if info.git_changes {
        notes.push(
            "- Files are sorted by Git change count (files with more changes are at the bottom)"
                .to_string(),
        );
    }
    if cfg.output.git.include_diffs {
        notes.push("- Git diffs from the worktree and staged changes are included".to_string());
    }
    if cfg.output.git.include_logs {
        let count = cfg.output.git.include_logs_count;
        notes.push(format!(
            "- Git logs ({count} commits) are included to show development patterns"
        ));
    }
    notes.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_for_entire_codebase() {
        let cfg = Config::default();
        let header = generate_header(&cfg);
        assert_eq!(
            header,
            "This file is a merged representation of the entire codebase, combined into a single document by Rustmix."
        );
    }

    #[test]
    fn header_for_subset() {
        let mut cfg = Config::default();
        cfg.include.push("src/**".to_string());
        let header = generate_header(&cfg);
        assert!(header.contains("a subset of the codebase, containing specifically included files"));
    }
}
