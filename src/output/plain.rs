//! Plain text output.

use super::RenderContext;

const PLAIN_SEPARATOR: &str = "================";
const PLAIN_LONG_SEPARATOR: &str = "================================================================";

/// Render the plain text output.
pub fn render(ctx: &RenderContext) -> String {
    let mut out = String::new();

    out.push('\n');

    if ctx.file_summary_enabled {
        out.push_str(&ctx.generation_header);
        out.push_str("\n\n");
        out.push_str(PLAIN_LONG_SEPARATOR);
        out.push_str("\nFile Summary\n");
        out.push_str(PLAIN_LONG_SEPARATOR);
        out.push_str("\n\nPurpose:\n--------\n");
        out.push_str(&ctx.summary_purpose);
        out.push_str("\n\nFile Format:\n------------\n");
        out.push_str(&ctx.summary_file_format);
        out.push_str(
            "\n5. Multiple file entries, each consisting of:\n  a. A separator line (================)\n  b. The file path (File: path/to/file)\n  c. Another separator line\n  d. The full contents of the file\n  e. A blank line\n\nUsage Guidelines:\n-----------------\n",
        );
        out.push_str(&ctx.summary_usage_guidelines);
        out.push_str("\n\nNotes:\n------\n");
        out.push_str(&ctx.summary_notes);
        out.push_str("\n\n");
    }

    out.push('\n');

    if let Some(header_text) = &ctx.header_text {
        if !header_text.is_empty() {
            out.push_str(PLAIN_LONG_SEPARATOR);
            out.push_str("\nUser Provided Header\n");
            out.push_str(PLAIN_LONG_SEPARATOR);
            out.push('\n');
            out.push_str(header_text);
            out.push_str("\n\n");
        }
    }

    if ctx.directory_structure_enabled {
        out.push_str(PLAIN_LONG_SEPARATOR);
        out.push_str("\nDirectory Structure\n");
        out.push_str(PLAIN_LONG_SEPARATOR);
        out.push('\n');
        out.push_str(&ctx.tree_string);
        out.push_str("\n\n");
    }

    if ctx.files_enabled {
        out.push_str(PLAIN_LONG_SEPARATOR);
        out.push_str("\nFiles\n");
        out.push_str(PLAIN_LONG_SEPARATOR);
        out.push_str("\n\n");
        for file in &ctx.processed_files {
            out.push_str(PLAIN_SEPARATOR);
            out.push_str("\nFile: ");
            out.push_str(&file.path);
            out.push('\n');
            out.push_str(PLAIN_SEPARATOR);
            out.push('\n');
            out.push_str(&file.content);
            out.push_str("\n\n");
        }
    }

    out.push('\n');

    if ctx.git_diff_enabled {
        out.push_str(PLAIN_LONG_SEPARATOR);
        out.push_str("\nGit Diffs\n");
        out.push_str(PLAIN_LONG_SEPARATOR);
        out.push('\n');
        out.push_str(PLAIN_SEPARATOR);
        out.push('\n');
        out.push_str(&ctx.git_diff_work_tree);
        out.push('\n');
        out.push_str(PLAIN_SEPARATOR);
        out.push_str("\n\n");
        out.push_str(PLAIN_SEPARATOR);
        out.push_str("\nGit Diffs Staged\n");
        out.push_str(PLAIN_SEPARATOR);
        out.push('\n');
        out.push_str(&ctx.git_diff_staged);
        out.push_str("\n\n");
    }

    out.push('\n');

    if ctx.git_log_enabled {
        out.push_str(PLAIN_LONG_SEPARATOR);
        out.push_str("\nGit Logs\n");
        out.push_str(PLAIN_LONG_SEPARATOR);
        out.push('\n');
        for commit in &ctx.git_log_commits {
            out.push_str(PLAIN_SEPARATOR);
            out.push_str("\nDate: ");
            out.push_str(&commit.date);
            out.push_str("\nMessage: ");
            out.push_str(&commit.message);
            out.push_str("\nFiles:\n");
            for file in &commit.files {
                out.push_str("  - ");
                out.push_str(file);
                out.push('\n');
            }
            out.push_str(PLAIN_SEPARATOR);
            out.push_str("\n\n");
        }
    }

    out.push('\n');

    if let Some(instruction) = &ctx.instruction {
        if !instruction.is_empty() {
            out.push_str(PLAIN_LONG_SEPARATOR);
            out.push_str("\nInstruction\n");
            out.push_str(PLAIN_LONG_SEPARATOR);
            out.push('\n');
            out.push_str(instruction);
            out.push('\n');
        }
    }

    out.push('\n');
    out.push_str(PLAIN_LONG_SEPARATOR);
    out.push_str("\nEnd of Codebase\n");
    out.push_str(PLAIN_LONG_SEPARATOR);
    out.push('\n');

    format!("{}\n", super::style_utils::js_trim(&out))
}
