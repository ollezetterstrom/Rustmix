//! Writing the packed output to disk, stdout or the clipboard.

use std::path::{Path, PathBuf};

use anyhow::{bail, Result};

use crate::cli::OutputStyle;
use crate::config::Config;
use crate::core::packager::{self, PackResult};
use crate::output;
use crate::util::path::locale_cmp;

/// A rendered output part.
pub struct OutputPart {
    pub file_path: PathBuf,
    pub content: String,
}

/// Build the file name of a split part.
pub fn build_split_output_file_path(base: &Path, part_index: usize) -> PathBuf {
    let ext = base.extension().map(|e| e.to_string_lossy().to_string());
    match ext {
        Some(ext) => {
            let stem = base
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let parent = base.parent().map(|p| p.to_path_buf()).unwrap_or_default();
            parent.join(format!("{stem}.{part_index}.{ext}"))
        }
        None => {
            let name = base
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let parent = base.parent().map(|p| p.to_path_buf()).unwrap_or_default();
            parent.join(format!("{name}.{part_index}"))
        }
    }
}

/// A group of files that belong together in one output part.
#[derive(Debug, Clone, Default)]
pub struct SplitGroup {
    pub root_entry: String,
    pub processed_files: Vec<crate::output::ProcessedFile>,
    pub all_file_paths: Vec<String>,
}

/// Split the processed files into groups keyed by their first path segment.
pub fn build_split_groups(
    processed_files: &[crate::output::ProcessedFile],
    all_file_paths: &[String],
) -> Vec<SplitGroup> {
    let mut groups: Vec<SplitGroup> = Vec::new();
    let mut index: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    let push = |groups: &mut Vec<SplitGroup>,
                    index: &mut std::collections::HashMap<String, usize>,
                    root_entry: String,
                    path: String,
                    processed: Option<&crate::output::ProcessedFile>| {
        let idx = match index.get(&root_entry) {
            Some(i) => *i,
            None => {
                groups.push(SplitGroup {
                    root_entry: root_entry.clone(),
                    ..Default::default()
                });
                let i = groups.len() - 1;
                index.insert(root_entry.clone(), i);
                i
            }
        };
        groups[idx].all_file_paths.push(path.clone());
        if let Some(p) = processed {
            groups[idx].processed_files.push(p.clone());
        }
    };

    for path in all_file_paths {
        let entry = path.split('/').next().unwrap_or(path).to_string();
        push(&mut groups, &mut index, entry, path.clone(), None);
    }
    for file in processed_files {
        let entry = file.path.split('/').next().unwrap_or(&file.path).to_string();
        push(
            &mut groups,
            &mut index,
            entry,
            file.path.clone(),
            Some(file),
        );
    }

    groups.sort_by(|a, b| locale_cmp(&a.root_entry, &b.root_entry));
    groups
}

/// Subdivide a group that is too large on its own.
fn subdivide_group(group: &SplitGroup) -> Option<Vec<SplitGroup>> {
    let child_depth = group.root_entry.split('/').count() + 1;
    let mut children: Vec<SplitGroup> = Vec::new();
    let mut index: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    for path in &group.all_file_paths {
        let prefix = path
            .split('/')
            .take(child_depth)
            .collect::<Vec<_>>()
            .join("/");
        let idx = match index.get(&prefix) {
            Some(i) => *i,
            None => {
                children.push(SplitGroup {
                    root_entry: prefix.clone(),
                    ..Default::default()
                });
                let i = children.len() - 1;
                index.insert(prefix.clone(), i);
                i
            }
        };
        children[idx].all_file_paths.push(path.clone());
    }
    for file in &group.processed_files {
        let prefix = file
            .path
            .split('/')
            .take(child_depth)
            .collect::<Vec<_>>()
            .join("/");
        let idx = match index.get(&prefix) {
            Some(i) => *i,
            None => {
                children.push(SplitGroup {
                    root_entry: prefix.clone(),
                    ..Default::default()
                });
                let i = children.len() - 1;
                index.insert(prefix.clone(), i);
                i
            }
        };
        children[idx].processed_files.push(file.clone());
    }

    children.sort_by(|a, b| locale_cmp(&a.root_entry, &b.root_entry));

    if children.len() == 1 && children[0].root_entry == group.root_entry {
        return None;
    }
    Some(children)
}

/// Render the output for a set of groups.
fn render_groups(
    groups: &[SplitGroup],
    part_index: usize,
    cfg: &Config,
    result: &PackResult,
) -> String {
    let mut processed: Vec<crate::output::ProcessedFile> = Vec::new();
    let mut all_paths: Vec<String> = Vec::new();
    for group in groups {
        processed.extend(group.processed_files.iter().cloned());
        all_paths.extend(group.all_file_paths.iter().cloned());
    }

    // The directory structure of a part is scoped to the files it contains.
    let tree_string = crate::core::tree::generate_tree_string(&all_paths, &[]);

    let instruction = None;
    let ctx = packager::build_render_context(
        cfg,
        &processed,
        &tree_string,
        &instruction,
        &crate::core::git::GitDiffResult::default(),
        &crate::core::git::GitLogResult::default(),
    );
    let _ = part_index;
    let _ = result;
    output::render(cfg.output.style, &ctx)
}

/// Split the output into multiple parts.
pub fn generate_split_parts(cfg: &Config, result: &PackResult) -> Result<Vec<OutputPart>> {
    let max_bytes = cfg.output.split_output.unwrap_or(0);
    if max_bytes == 0 {
        bail!("Invalid split size");
    }
    let base_path = packager::resolve_path(&cfg.cwd.join(&cfg.output.file_path));
    let mut groups = build_split_groups(&result.processed_files, &[]);
    let mut queue: std::collections::VecDeque<SplitGroup> = groups.drain(..).collect();
    let mut parts: Vec<OutputPart> = Vec::new();

    while let Some(group) = queue.pop_front() {
        let mut current: Vec<SplitGroup> = Vec::new();
        let mut candidate = group.clone();
        loop {
            let mut trial = current.clone();
            trial.push(candidate.clone());
            let content = render_groups(&trial, parts.len() + 1, cfg, result);
            if content.len() as u64 <= max_bytes {
                current = trial;
                break;
            }
            if !current.is_empty() {
                // Finalize the current part and retry the group on its own.
                let content = render_groups(&current, parts.len() + 1, cfg, result);
                parts.push(OutputPart {
                    file_path: build_split_output_file_path(&base_path, parts.len() + 1),
                    content,
                });
                break;
            }
            // The group alone is too large: subdivide it.
            match subdivide_group(&candidate) {
                Some(children) => {
                    for child in children.into_iter().rev() {
                        queue.push_front(child);
                    }
                    candidate = SplitGroup::default();
                    break;
                }
                None => {
                    bail!(
                        "Cannot split output: '{}' exceeds max size on its own. Size {} bytes > limit {} bytes. A single file cannot be split across parts.",
                        group.root_entry,
                        content.len(),
                        max_bytes
                    );
                }
            }
        }
        if !current.is_empty() {
            let content = render_groups(&current, parts.len() + 1, cfg, result);
            parts.push(OutputPart {
                file_path: build_split_output_file_path(&base_path, parts.len() + 1),
                content,
            });
        }
    }

    Ok(parts)
}

/// Write the output to the configured destination.
pub fn write_output(cfg: &Config, content: &str) -> Result<Vec<PathBuf>> {
    if cfg.output.stdout {
        use std::io::Write;
        let stdout = std::io::stdout();
        let mut lock = stdout.lock();
        lock.write_all(content.as_bytes())?;
        lock.flush()?;
        return Ok(Vec::new());
    }

    let path = packager::resolve_path(&cfg.cwd.join(&cfg.output.file_path));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, content.as_bytes())?;
    Ok(vec![path])
}

/// Copy the given text to the system clipboard.
pub fn copy_to_clipboard(content: &str) -> Result<()> {
    let mut clipboard = arboard::Clipboard::new()?;
    clipboard.set_text(content.to_string())?;
    Ok(())
}

/// The default output file name for a style.
pub fn default_file_name(style: OutputStyle) -> &'static str {
    style.default_file_name()
}
