//! JSON output.

use serde_json::{Map, Value};

use super::RenderContext;

/// The file format description used by the JSON output.
pub fn summary_file_format_json() -> String {
    "\nThe content is organized as follows:\n1. This summary section\n2. Repository information\n3. Directory structure\n4. Repository files, each consisting of:\n   - File path as a key\n   - Full contents of the file as the value\n"
        .trim()
        .to_string()
}

/// Render the JSON output.
pub fn render(ctx: &RenderContext<'_>) -> String {
    let mut root = Map::new();

    if ctx.file_summary_enabled {
        let mut summary = Map::new();
        summary.insert(
            "generationHeader".to_string(),
            Value::String(ctx.generation_header.clone()),
        );
        summary.insert("purpose".to_string(), Value::String(ctx.summary_purpose.clone()));
        summary.insert(
            "fileFormat".to_string(),
            Value::String(summary_file_format_json()),
        );
        summary.insert(
            "usageGuidelines".to_string(),
            Value::String(ctx.summary_usage_guidelines.clone()),
        );
        summary.insert("notes".to_string(), Value::String(ctx.summary_notes.clone()));
        root.insert("fileSummary".to_string(), Value::Object(summary));
    }

    if let Some(header_text) = &ctx.header_text {
        if !header_text.is_empty() {
            root.insert(
                "userProvidedHeader".to_string(),
                Value::String(header_text.clone()),
            );
        }
    }

    if ctx.directory_structure_enabled {
        root.insert(
            "directoryStructure".to_string(),
            Value::String(ctx.tree_string.clone()),
        );
    }

    if ctx.files_enabled {
        let mut files = Map::new();
        for file in ctx.processed_files {
            files.insert(file.path.clone(), Value::String(file.content.clone()));
        }
        root.insert("files".to_string(), Value::Object(files));
    }

    if ctx.git_diff_enabled {
        let mut diffs = Map::new();
        diffs.insert(
            "workTree".to_string(),
            Value::String(ctx.git_diff_work_tree.clone()),
        );
        diffs.insert("staged".to_string(), Value::String(ctx.git_diff_staged.clone()));
        root.insert("gitDiffs".to_string(), Value::Object(diffs));
    }

    if ctx.git_log_enabled {
        let mut logs = Vec::new();
        for commit in &ctx.git_log_commits {
            let mut entry = Map::new();
            entry.insert("date".to_string(), Value::String(commit.date.clone()));
            entry.insert("message".to_string(), Value::String(commit.message.clone()));
            entry.insert(
                "files".to_string(),
                Value::Array(
                    commit
                        .files
                        .iter()
                        .map(|f| Value::String(f.clone()))
                        .collect(),
                ),
            );
            logs.push(Value::Object(entry));
        }
        root.insert("gitLogs".to_string(), Value::Array(logs));
    }

    if let Some(instruction) = &ctx.instruction {
        if !instruction.is_empty() {
            root.insert(
                "instruction".to_string(),
                Value::String(instruction.clone()),
            );
        }
    }

    serde_json::to_string_pretty(&Value::Object(root)).unwrap_or_default()
}
