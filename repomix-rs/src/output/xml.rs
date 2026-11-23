//! XML output.
//!
//! The layout mirrors the Handlebars template used by the original
//! implementation. Handlebars removes the line of a "standalone" block
//! helper (`{{#if}}`, `{{/if}}`, `{{#each}}`, `{{/each}}`), which is why the
//! section blocks below are concatenated without separators.

use super::style_utils::escape_xml;
use super::RenderContext;

/// Render the default (Handlebars style) XML output.
pub fn render(ctx: &RenderContext<'_>) -> String {
    let mut out = String::with_capacity(super::style_utils::estimate_output_size(
        ctx.processed_files,
        &ctx.tree_string,
        ctx.git_diff_work_tree.len() + ctx.git_diff_staged.len(),
    ));

    out.push('\n');

    if ctx.file_summary_enabled {
        out.push_str(&ctx.generation_header);
        out.push_str("\n\n<file_summary>\nThis section contains a summary of this file.\n\n<purpose>\n");
        out.push_str(&ctx.summary_purpose);
        out.push_str("\n</purpose>\n\n<file_format>\n");
        out.push_str(&ctx.summary_file_format);
        out.push_str(
            "\n5. Multiple file entries, each consisting of:\n  - File path as an attribute\n  - Full contents of the file\n</file_format>\n\n<usage_guidelines>\n",
        );
        out.push_str(&ctx.summary_usage_guidelines);
        out.push_str("\n</usage_guidelines>\n\n<notes>\n");
        out.push_str(&ctx.summary_notes);
        out.push_str("\n</notes>\n\n</file_summary>\n\n");
    }

    if let Some(header_text) = &ctx.header_text {
        if !header_text.is_empty() {
            out.push_str("<user_provided_header>\n");
            out.push_str(header_text);
            out.push_str("\n</user_provided_header>\n\n");
        }
    }

    if ctx.directory_structure_enabled {
        out.push_str("<directory_structure>\n");
        out.push_str(&ctx.tree_string);
        out.push_str("\n</directory_structure>\n\n");
    }

    if ctx.files_enabled {
        out.push_str("<files>\nThis section contains the contents of the repository's files.\n\n");
        for file in ctx.processed_files {
            out.push_str("<file path=\"");
            out.push_str(&file.path);
            out.push_str("\">\n");
            out.push_str(&file.content);
            out.push_str("\n</file>\n\n");
        }
        out.push_str("</files>\n");
    }

    out.push('\n');

    if ctx.git_diff_enabled {
        out.push_str("<git_diffs>\n<git_diff_work_tree>\n");
        out.push_str(&ctx.git_diff_work_tree);
        out.push_str("\n</git_diff_work_tree>\n<git_diff_staged>\n");
        out.push_str(&ctx.git_diff_staged);
        out.push_str("\n</git_diff_staged>\n</git_diffs>\n");
    }

    out.push('\n');

    if ctx.git_log_enabled {
        out.push_str("<git_logs>\n");
        for commit in &ctx.git_log_commits {
            out.push_str("<git_log_commit>\n<date>");
            out.push_str(&commit.date);
            out.push_str("</date>\n<message>");
            out.push_str(&commit.message);
            out.push_str("</message>\n<files>\n");
            for file in &commit.files {
                out.push_str(file);
                out.push('\n');
            }
            out.push_str("</files>\n</git_log_commit>\n");
        }
        out.push_str("</git_logs>\n");
    }

    out.push('\n');

    if let Some(instruction) = &ctx.instruction {
        if !instruction.is_empty() {
            out.push_str("<instruction>\n");
            out.push_str(instruction);
            out.push_str("\n</instruction>\n");
        }
    }

    super::style_utils::finalize_trimmed_output(out)
}

/// Render the "parsable" XML output (built the way `fast-xml-builder` does).
pub fn render_parsable(ctx: &RenderContext<'_>) -> String {
    let mut out = String::with_capacity(super::style_utils::estimate_output_size(
        ctx.processed_files,
        &ctx.tree_string,
        ctx.git_diff_work_tree.len() + ctx.git_diff_staged.len(),
    ));
    out.push_str("<rustmix>");
    if ctx.file_summary_enabled {
        out.push_str("<file_summary>");
        out.push_str(&escape_xml(&ctx.generation_header));
        out.push_str("<purpose>");
        out.push_str(&escape_xml(&ctx.summary_purpose));
        out.push_str("</purpose>");
        out.push_str("<file_format>");
        out.push_str(&escape_xml(&ctx.summary_file_format));
        out.push_str("\n5. Repository files, each consisting of:\n  - File path as an attribute\n  - Full contents of the file");
        out.push_str("</file_format>");
        out.push_str("<usage_guidelines>");
        out.push_str(&escape_xml(&ctx.summary_usage_guidelines));
        out.push_str("</usage_guidelines>");
        out.push_str("<notes>");
        out.push_str(&escape_xml(&ctx.summary_notes));
        out.push_str("</notes>");
        out.push_str("</file_summary>");
    }
    if let Some(header_text) = &ctx.header_text {
        if !header_text.is_empty() {
            out.push_str("<user_provided_header>");
            out.push_str(&escape_xml(header_text));
            out.push_str("</user_provided_header>");
        }
    }
    if ctx.directory_structure_enabled {
        out.push_str("<directory_structure>");
        out.push_str(&escape_xml(&ctx.tree_string));
        out.push_str("</directory_structure>");
    }
    if ctx.files_enabled {
        out.push_str("<files>");
        out.push_str("This section contains the contents of the repository&apos;s files.");
        for file in ctx.processed_files {
            out.push_str("<file path=\"");
            out.push_str(&escape_xml(&file.path));
            out.push_str("\">");
            out.push_str(&escape_xml(&file.content));
            out.push_str("</file>");
        }
        out.push_str("</files>");
    }
    if ctx.git_diff_enabled {
        out.push_str("<git_diffs><git_diff_work_tree>");
        out.push_str(&escape_xml(&ctx.git_diff_work_tree));
        out.push_str("</git_diff_work_tree><git_diff_staged>");
        out.push_str(&escape_xml(&ctx.git_diff_staged));
        out.push_str("</git_diff_staged></git_diffs>");
    }
    if ctx.git_log_enabled {
        out.push_str("<git_logs>");
        for commit in &ctx.git_log_commits {
            out.push_str("<git_log_commit><date>");
            out.push_str(&escape_xml(&commit.date));
            out.push_str("</date><message>");
            out.push_str(&escape_xml(&commit.message));
            out.push_str("</message><files>");
            for file in &commit.files {
                out.push_str("<files>");
                out.push_str(&escape_xml(file));
                out.push_str("</files>");
            }
            out.push_str("</files></git_log_commit>");
        }
        out.push_str("</git_logs>");
    }
    if let Some(instruction) = &ctx.instruction {
        if !instruction.is_empty() {
            out.push_str("<instruction>");
            out.push_str(&escape_xml(instruction));
            out.push_str("</instruction>");
        }
    }
    out.push_str("</rustmix>");
    out
}
