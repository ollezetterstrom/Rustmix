//! Markdown output.

use super::style_utils::language_from_file_path;
use super::RenderContext;

/// Render the markdown output.
pub fn render(ctx: &RenderContext<'_>) -> String {
    let mut out = String::with_capacity(super::style_utils::estimate_output_size(
        ctx.processed_files,
        &ctx.tree_string,
        ctx.git_diff_work_tree.len() + ctx.git_diff_staged.len(),
    ));
    let delim = &ctx.markdown_code_block_delimiter;

    out.push('\n');

    if ctx.file_summary_enabled {
        out.push_str(&ctx.generation_header);
        out.push_str("\n\n# File Summary\n\n## Purpose\n");
        out.push_str(&ctx.summary_purpose);
        out.push_str("\n\n## File Format\n");
        out.push_str(&ctx.summary_file_format);
        out.push_str(
            "\n5. Multiple file entries, each consisting of:\n  a. A header with the file path (## File: path/to/file)\n  b. The full contents of the file in a code block\n\n## Usage Guidelines\n",
        );
        out.push_str(&ctx.summary_usage_guidelines);
        out.push_str("\n\n## Notes\n");
        out.push_str(&ctx.summary_notes);
        out.push_str("\n\n");
    }

    if let Some(header_text) = &ctx.header_text {
        if !header_text.is_empty() {
            out.push_str("# User Provided Header\n");
            out.push_str(header_text);
            out.push_str("\n\n");
        }
    }

    if ctx.directory_structure_enabled {
        out.push_str("# Directory Structure\n");
        out.push_str(delim);
        out.push('\n');
        out.push_str(&ctx.tree_string);
        out.push('\n');
        out.push_str(delim);
        out.push_str("\n\n");
    }

    if ctx.files_enabled {
        out.push_str("# Files\n\n");
        for file in ctx.processed_files {
            out.push_str("## File: ");
            out.push_str(&file.path);
            out.push('\n');
            out.push_str(delim);
            out.push_str(language_from_file_path(&file.path));
            out.push('\n');
            out.push_str(&file.content);
            out.push('\n');
            out.push_str(delim);
            out.push_str("\n\n");
        }
    }

    out.push('\n');

    if ctx.git_diff_enabled {
        out.push_str("# Git Diffs\n## Git Diffs Working Tree\n");
        out.push_str(delim);
        out.push_str("diff\n");
        out.push_str(&ctx.git_diff_work_tree);
        out.push('\n');
        out.push_str(delim);
        out.push_str("\n\n## Git Diffs Staged\n");
        out.push_str(delim);
        out.push_str("diff\n");
        out.push_str(&ctx.git_diff_staged);
        out.push('\n');
        out.push_str(delim);
        out.push_str("\n\n");
    }

    out.push('\n');

    if ctx.git_log_enabled {
        out.push_str("# Git Logs\n\n");
        for commit in &ctx.git_log_commits {
            out.push_str("## Commit: ");
            out.push_str(&commit.date);
            out.push_str("\n**Message:** ");
            out.push_str(&commit.message);
            out.push_str("\n\n**Files:**\n");
            for file in &commit.files {
                out.push_str("- ");
                out.push_str(file);
                out.push('\n');
            }
            out.push('\n');
        }
    }

    out.push('\n');

    if let Some(instruction) = &ctx.instruction {
        if !instruction.is_empty() {
            out.push_str("# Instruction\n");
            out.push_str(instruction);
            out.push('\n');
        }
    }

    super::style_utils::finalize_trimmed_output(out)
}
