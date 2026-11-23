//! Compact LLM-first output style.

use std::collections::HashMap;
use std::io::Write;

use crate::core::metrics::fingerprint;
use super::RenderContext;

/// Stream compact representation directly into a writer.
pub fn render_to_writer<W: Write>(ctx: &RenderContext<'_>, writer: &mut W) -> std::io::Result<()> {
    if let Some(header) = &ctx.header_text {
        if !header.is_empty() {
            writer.write_all(header.as_bytes())?;
            writer.write_all(b"\n\n")?;
        }
    }
    if let Some(instruction) = &ctx.instruction {
        if !instruction.is_empty() {
            writer.write_all(instruction.as_bytes())?;
            writer.write_all(b"\n\n")?;
        }
    }

    if ctx.directory_structure_enabled && !ctx.tree_string.is_empty() {
        writer.write_all(ctx.tree_string.as_bytes())?;
        if !ctx.tree_string.ends_with('\n') {
            writer.write_all(b"\n")?;
        }
        writer.write_all(b"\n")?;
    }

    if ctx.files_enabled {
        let mut buckets: HashMap<(usize, u64, u64), Vec<usize>> =
            HashMap::with_capacity(ctx.processed_files.len());

        for (i, file) in ctx.processed_files.iter().enumerate() {
            let key = fingerprint(file.content.as_bytes());
            let bucket = buckets.entry(key).or_default();
            let mut duplicate_of = None;
            for &candidate_idx in bucket.iter() {
                if ctx.processed_files[candidate_idx].content == file.content {
                    duplicate_of = Some(candidate_idx);
                    break;
                }
            }

            if let Some(original_idx) = duplicate_of {
                let original_path = &ctx.processed_files[original_idx].path;
                writeln!(writer, "=== {} (duplicate of {})", file.path, original_path)?;
                writer.write_all(b"\n")?;
            } else {
                bucket.push(i);
                writeln!(writer, "=== {}", file.path)?;
                writer.write_all(file.content.as_bytes())?;
                if !file.content.ends_with('\n') {
                    writer.write_all(b"\n")?;
                }
                writer.write_all(b"\n")?;
            }
        }
    }

    if ctx.git_diff_enabled {
        if !ctx.git_diff_work_tree.is_empty() || !ctx.git_diff_staged.is_empty() {
            writer.write_all(b"=== git diff ===\n")?;
            if !ctx.git_diff_work_tree.is_empty() {
                writer.write_all(ctx.git_diff_work_tree.as_bytes())?;
                writer.write_all(b"\n")?;
            }
            if !ctx.git_diff_staged.is_empty() {
                writer.write_all(ctx.git_diff_staged.as_bytes())?;
                writer.write_all(b"\n")?;
            }
        }
    }

    if ctx.git_log_enabled && !ctx.git_log_commits.is_empty() {
        writer.write_all(b"=== git log ===\n")?;
        for commit in &ctx.git_log_commits {
            writeln!(writer, "{} {}", commit.date, commit.message)?;
            for f in &commit.files {
                writeln!(writer, "  {}", f)?;
            }
        }
        writer.write_all(b"\n")?;
    }

    Ok(())
}

/// Render the compact output to an in-memory String.
pub fn render(ctx: &RenderContext<'_>) -> String {
    let mut buf = Vec::new();
    render_to_writer(ctx, &mut buf).expect("writing to memory buffer");
    String::from_utf8(buf).expect("valid utf-8 output")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::ProcessedFile;

    #[test]
    fn renders_compact_output_with_deduplication() {
        let files = vec![
            ProcessedFile {
                path: "src/a.rs".to_string(),
                content: "fn main() {}".to_string(),
            },
            ProcessedFile {
                path: "src/b.rs".to_string(),
                content: "fn main() {}".to_string(),
            },
            ProcessedFile {
                path: "src/c.rs".to_string(),
                content: "fn test() {}".to_string(),
            },
        ];
        let ctx = RenderContext {
            tree_string: "src/\n  a.rs\n  b.rs\n  c.rs\n".to_string(),
            processed_files: &files,
            directory_structure_enabled: true,
            files_enabled: true,
            ..Default::default()
        };

        let rendered = render(&ctx);
        assert!(rendered.contains("src/\n  a.rs\n  b.rs\n  c.rs\n"));
        assert!(rendered.contains("=== src/a.rs\nfn main() {}\n"));
        assert!(rendered.contains("=== src/b.rs (duplicate of src/a.rs)\n"));
        assert!(rendered.contains("=== src/c.rs\nfn test() {}\n"));
        // Boilerplate omitted
        assert!(!rendered.contains("<file_summary>"));
        assert!(!rendered.contains("Usage Guidelines"));
    }
}
