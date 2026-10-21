//! The packing pipeline: search, read, check, process, sort, render.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use rayon::prelude::*;

use crate::config::Config;
use crate::core::comments;
use crate::core::compress;
use crate::core::decorations;
use crate::core::git::{self, GitDiffResult, GitLogResult};
use crate::core::metrics::{FileMetrics, Metrics};
use crate::core::read::{self, RawFile, SkipReason};
use crate::core::search;
use crate::core::security::{self, SecurityResult};
use crate::core::tree;
use crate::output::{self, ProcessedFile, RenderContext};
use crate::util::path::{sort_paths, to_posix};

/// A file that was skipped while collecting the output.
#[derive(Debug, Clone)]
pub struct SkippedFile {
    pub path: String,
    pub reason: SkipReason,
}

/// The result of packing a repository.
#[derive(Debug, Default)]
pub struct PackResult {
    pub output: String,
    pub output_files: Vec<PathBuf>,
    pub processed_files: Vec<ProcessedFile>,
    pub metrics: Metrics,
    pub suspicious_files: Vec<SecurityResult>,
    pub skipped_files: Vec<SkippedFile>,
    pub git_diff_tokens: usize,
    pub git_log_tokens: usize,
    /// The output of each split part (empty when not splitting).
    pub output_parts: Vec<String>,
}

/// Files grouped per root directory.
struct RootFiles {
    root_dir: PathBuf,
    /// Root relative paths.
    file_paths: Vec<String>,
    /// Display paths.
    display_paths: Vec<String>,
}

/// Pack the given root directories.
pub fn pack(
    root_dirs: &[PathBuf],
    cfg: &Config,
    progress: &mut dyn FnMut(&str),
) -> Result<PackResult> {
    progress("Searching for files...");
    let search_results: Vec<search::SearchOutcome> = root_dirs
        .par_iter()
        .map(|root| search::search_files(root, cfg))
        .collect::<Result<Vec<_>>>()?;

    let file_path_style = cfg.output.file_path_style;
    let uses_root_labels = matches!(
        file_path_style,
        crate::cli::FilePathStyle::TargetRelative
    );
    let root_labels = if uses_root_labels && root_dirs.len() > 1 {
        Some(build_root_labels(root_dirs, &cfg.cwd))
    } else {
        None
    };

    let mut empty_dir_paths: Vec<String> = Vec::new();
    if cfg.output.include_empty_directories.unwrap_or(false) {
        for (index, root) in root_dirs.iter().enumerate() {
            for dir in &search_results[index].empty_dir_paths {
                let label = root_labels.as_ref().and_then(|l| l.get(index));
                empty_dir_paths.push(build_file_display_path(
                    root,
                    dir,
                    &cfg.cwd,
                    file_path_style,
                    label,
                ));
            }
        }
        empty_dir_paths.sort();
        empty_dir_paths.dedup();
    }

    progress("Sorting files...");
    let root_files: Vec<RootFiles> = root_dirs
        .iter()
        .enumerate()
        .map(|(index, root)| {
            let mut file_paths = search_results[index].file_paths.clone();
            file_paths.dedup();
            sort_paths(&mut file_paths);
            let display_paths = file_paths
                .iter()
                .map(|p| {
                    build_file_display_path(
                        root,
                        p,
                        &cfg.cwd,
                        file_path_style,
                        root_labels.as_ref().and_then(|l| l.get(index)),
                    )
                })
                .collect();
            RootFiles {
                root_dir: root.clone(),
                file_paths,
                display_paths,
            }
        })
        .collect();

    let all_file_paths: Vec<String> = root_files
        .iter()
        .flat_map(|r| r.display_paths.clone())
        .collect();

    // --- Collect and read the files -------------------------------------
    progress("Collecting files...");
    let mut raw_files: Vec<RawFile> = Vec::new();
    let mut skipped_files: Vec<SkippedFile> = Vec::new();
    for root_files in &root_files {
        let root = &root_files.root_dir;
        let collected: Vec<RawFile> = root_files
            .file_paths
            .par_iter()
            .map(|rel| read::read_raw_file(&root.join(rel), rel, cfg.max_file_size))
            .collect();
        for file in collected {
            match (&file.content, file.skipped) {
                (Some(_), _) => raw_files.push(file),
                (None, Some(reason)) => skipped_files.push(SkippedFile {
                    path: file.path.clone(),
                    reason,
                }),
                (None, None) => {}
            }
        }
    }

    // --- Git integration -------------------------------------------------
    let git_diff_result = if cfg.output.git.include_diffs {
        git::get_diffs(root_dirs.first().map(|p| p.as_path()).unwrap_or(Path::new(".")))
    } else {
        GitDiffResult::default()
    };
    let git_log_result = if cfg.output.git.include_logs {
        git::get_logs(
            root_dirs.first().map(|p| p.as_path()).unwrap_or(Path::new(".")),
            cfg.output.git.include_logs_count,
        )
    } else {
        GitLogResult::default()
    };

    // --- Security check --------------------------------------------------
    let mut suspicious_files: Vec<SecurityResult> = Vec::new();
    if cfg.security_check_enabled {
        progress("Running security check...");
        let results: Vec<SecurityResult> = raw_files
            .par_iter()
            .filter_map(|file| {
                file.content
                    .as_ref()
                    .and_then(|c| security::scan_file(&file.path, c))
            })
            .collect();
        suspicious_files = results;    }

    let suspicious_paths: std::collections::HashSet<String> =
        suspicious_files.iter().map(|r| r.file_path.clone()).collect();

    // --- Process the content ---------------------------------------------
    progress("Processing files...");
    let processed = raw_files
        .par_iter()
        .filter_map(|file| {
            let content = file.content.as_ref()?;
            if suspicious_paths.contains(&file.path) {
                return None;
            }
            let processed_content = process_content(&file.path, content, cfg);
            Some(ProcessedFile {
                path: file.path.clone(),
                content: processed_content,
            })
        })
        .collect::<Vec<_>>();

    // --- Sort by git change frequency ------------------------------------
    let mut processed = processed;
    if cfg.output.git.sort_by_changes {
        let counts = git::get_change_counts(
            root_dirs.first().map(|p| p.as_path()).unwrap_or(Path::new(".")),
            cfg.output.git.sort_by_changes_max_commits,
        );
        processed.sort_by_key(|f| counts.get(&f.path).copied().unwrap_or(0));
    }

    // --- Build the directory tree -----------------------------------------
    let tree_string = build_tree_string(&root_files, &root_labels, &all_file_paths, &empty_dir_paths, cfg);

    // --- Render ------------------------------------------------------------
    progress("Generating output...");
    let instruction = load_instruction(cfg)?;
    let ctx = build_render_context(cfg, &processed, &tree_string, &instruction, &git_diff_result, &git_log_result);

    let output = output::render(cfg.output.style, &ctx);

    // --- Metrics ------------------------------------------------------------
    progress("Calculating metrics...");
    let file_metrics = crate::core::metrics::calculate_file_metrics(
        &processed
            .iter()
            .map(|f| (f.path.clone(), f.content.clone()))
            .collect::<Vec<_>>(),
        cfg.token_encoding,
    );
    let total_characters: usize = output.chars().count();
    let total_tokens = crate::core::metrics::count_tokens_chunked(&output, cfg.token_encoding);
    let git_diff_tokens = crate::core::metrics::count_tokens_chunked(
        &format!(
            "{}{}",
            git_diff_result.work_tree_diff_content, git_diff_result.staged_diff_content
        ),
        cfg.token_encoding,
    );
    let git_log_tokens = crate::core::metrics::count_tokens_chunked(&git_log_result.log_content, cfg.token_encoding);

    let metrics = Metrics {
        total_files: processed.len(),
        total_characters,
        total_tokens,
        file_metrics,
        git_diff_tokens,
        git_log_tokens,
    };

    Ok(PackResult {
        output,
        output_files: Vec::new(),
        processed_files: processed,
        metrics,
        suspicious_files,
        skipped_files,
        git_diff_tokens,
        git_log_tokens,
        output_parts: Vec::new(),
    })
}

/// Apply the content transformations in Repomix's order.
pub fn process_content(path: &str, content: &str, cfg: &Config) -> String {
    let ext = extension_of(path);
    let mut content = content.to_string();

    // 1. Remove comments.
    if cfg.output.remove_comments {
        if let Some(stripped) = comments::remove_comments(&content, &ext) {
            content = stripped;
        }
    }

    // 2. Compress.
    if cfg.output.compress {
        if let Some(compressed) = compress::compress_file(path, &content) {
            content = compressed;
        }
    }

    // 3. Truncate base64.
    if cfg.output.truncate_base64 {
        content = crate::core::base64::truncate_base64_content(&content);
    }

    // 4. Remove empty lines (only for supported file types).
    if cfg.output.remove_empty_lines && comments::supports_comments(&ext) {
        content = comments::remove_empty_lines(&content);
    }

    // 5. Trim.
    content = output::style_utils::js_trim(&content).to_string();

    // 6. Add line numbers.
    if cfg.output.show_line_numbers {
        let lines: Vec<&str> = content.split('\n').collect();
        let padding = lines.len().to_string().len();
        let numbered: Vec<String> = lines
            .iter()
            .enumerate()
            .map(|(i, line)| format!("{:>width$}: {}", i + 1, line, width = padding))
            .collect();
        content = numbered.join("\n");
    }

    content
}

/// The lowercase extension of a path (without the dot).
pub fn extension_of(path: &str) -> String {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => ext.to_ascii_lowercase(),
        _ => String::new(),
    }
}

/// Build the labels used for multiple root directories.
pub fn build_root_labels(root_dirs: &[PathBuf], cwd: &Path) -> Vec<String> {
    let mut labels: Vec<String> = root_dirs
        .iter()
        .map(|root| {
            let resolved = resolve_path(root);
            let relative = path_relative(cwd, &resolved);
            if is_cwd_relative(&relative) {
                to_posix(&relative)
            } else {
                let base = resolved
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| resolved.to_string_lossy().to_string());
                to_posix(&base)
            }
        })
        .collect();

    // Uniquify the labels.
    let mut seen: Vec<String> = Vec::new();
    for (index, label) in labels.iter_mut().enumerate() {
        if label.is_empty() {
            *label = format!("root-{}", index + 1);
        }
        let base = label.clone();
        let mut candidate = base.clone();
        let mut suffix = 2;
        while seen.contains(&candidate) {
            candidate = format!("{base}-{suffix}");
            suffix += 1;
        }
        seen.push(candidate.clone());
        *label = candidate;
    }
    labels
}

fn is_cwd_relative(relative: &str) -> bool {
    !relative.is_empty()
        && relative != ".."
        && !relative.starts_with("../")
        && !Path::new(relative).is_absolute()
}

/// Join a root label and a file path.
pub fn join_display_path(root_label: &str, file_path: &str) -> String {
    let mut label = to_posix(root_label);
    label = label.trim_matches('/').to_string();
    if label.is_empty() {
        label = "root".to_string();
    }
    let file = to_posix(file_path);
    let file = file.trim_start_matches('/').to_string();
    if file.is_empty() {
        label
    } else {
        format!("{label}/{file}")
    }
}

/// Build the display path of a file.
pub fn build_file_display_path(
    root_dir: &Path,
    file_path: &str,
    cwd: &Path,
    style: crate::cli::FilePathStyle,
    root_label: Option<&String>,
) -> String {
    match style {
        crate::cli::FilePathStyle::CwdRelative => {
            let absolute = resolve_path(&root_dir.join(file_path));
            let rel = path_relative(cwd, &absolute);
            let rel = to_posix(&rel);
            if rel.is_empty() {
                ".".to_string()
            } else {
                rel
            }
        }
        crate::cli::FilePathStyle::TargetRelative => match root_label {
            Some(label) => join_display_path(label, file_path),
            None => to_posix(file_path),
        },
    }
}

/// Resolve a path against the current directory.
pub fn resolve_path(path: &Path) -> PathBuf {
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    // Drop `.` components so that the path can be compared as a prefix.
    let mut out = PathBuf::new();
    for component in joined.components() {
        match component {
            std::path::Component::CurDir => {}
            c => out.push(c.as_os_str()),
        }
    }
    out
}

/// Compute a relative path using POSIX separators.
pub fn path_relative(base: &Path, target: &Path) -> String {
    let base = base.to_string_lossy().to_string();
    let target = target.to_string_lossy().to_string();
    let base = base.replace('\\', "/");
    let target = target.replace('\\', "/");
    let base_trim = base.trim_end_matches('/');
    if target == base_trim {
        return String::new();
    }
    if let Some(rest) = target.strip_prefix(&format!("{base_trim}/")) {
        return rest.to_string();
    }
    // Fall back to a component-wise comparison.
    let base_parts: Vec<&str> = base_trim.split('/').filter(|s| !s.is_empty()).collect();
    let target_parts: Vec<&str> = target.split('/').filter(|s| !s.is_empty()).collect();
    let mut common = 0usize;
    while common < base_parts.len()
        && common < target_parts.len()
        && base_parts[common] == target_parts[common]
    {
        common += 1;
    }
    let mut parts: Vec<String> = Vec::new();
    for _ in common..base_parts.len() {
        parts.push("..".to_string());
    }
    for c in &target_parts[common..] {
        parts.push((*c).to_string());
    }
    parts.join("/")
}

/// Build the directory structure section.
fn build_tree_string(
    root_files: &[RootFiles],
    root_labels: &Option<Vec<String>>,
    all_file_paths: &[String],
    empty_dir_paths: &[String],
    cfg: &Config,
) -> String {
    let use_full_tree = cfg.output.directory_structure
        && cfg.output.include_full_directory_structure
        && !cfg.include.is_empty();

    if use_full_tree {
        let mut dirs: Vec<String> = Vec::new();
        let mut files: Vec<String> = Vec::new();
        for (index, root_files) in root_files.iter().enumerate() {
            let label = root_labels.as_ref().and_then(|l| l.get(index));
            if let Ok(list) = search::list_directories(&root_files.root_dir, cfg) {
                for d in list {
                    dirs.push(build_file_display_path(
                        &root_files.root_dir,
                        &d,
                        &cfg.cwd,
                        cfg.output.file_path_style,
                        label,
                    ));
                }
            }
            if let Ok(list) = search::list_files(&root_files.root_dir, cfg) {
                for f in list {
                    files.push(build_file_display_path(
                        &root_files.root_dir,
                        &f,
                        &cfg.cwd,
                        cfg.output.file_path_style,
                        label,
                    ));
                }
            }
        }
        dirs.sort();
        dirs.dedup();
        let mut tree_files = all_file_paths.to_vec();
        for f in files {
            if !tree_files.contains(&f) {
                tree_files.push(f);
            }
        }
        return tree::generate_tree_string(&tree_files, &dirs);
    }

    let uses_root_labels = matches!(
        cfg.output.file_path_style,
        crate::cli::FilePathStyle::TargetRelative
    );
    if uses_root_labels {
        let by_root: Vec<(String, Vec<String>)> = root_files
            .iter()
            .enumerate()
            .map(|(index, rf)| {
                let label = root_labels
                    .as_ref()
                    .and_then(|l| l.get(index))
                    .cloned()
                    .unwrap_or_else(|| {
                        rf.root_dir
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| rf.root_dir.to_string_lossy().to_string())
                    });
                (label, rf.file_paths.clone())
            })
            .collect();
        tree::generate_tree_string_with_roots(&by_root, empty_dir_paths)
    } else {
        tree::generate_tree_string(all_file_paths, empty_dir_paths)
    }
}

/// Build the render context.
pub fn build_render_context(
    cfg: &Config,
    processed_files: &[ProcessedFile],
    tree_string: &str,
    instruction: &Option<String>,
    git_diff_result: &GitDiffResult,
    git_log_result: &GitLogResult,
) -> RenderContext {
    let has_instruction = instruction.is_some();
    let mut contents: Vec<&str> = processed_files.iter().map(|f| f.content.as_str()).collect();
    contents.push(tree_string);
    contents.push(&git_diff_result.work_tree_diff_content);
    contents.push(&git_diff_result.staged_diff_content);
    let delimiter = output::calculate_markdown_delimiter(&contents);

    RenderContext {
        generation_header: decorations::generate_header(cfg),
        summary_purpose: decorations::generate_summary_purpose(cfg),
        summary_file_format: decorations::generate_summary_file_format(),
        summary_usage_guidelines: decorations::generate_summary_usage_guidelines(cfg, has_instruction),
        summary_notes: decorations::generate_summary_notes(cfg),
        header_text: cfg.output.header_text.clone(),
        instruction: instruction.clone(),
        tree_string: tree_string.to_string(),
        processed_files: processed_files.to_vec(),
        file_summary_enabled: cfg.output.file_summary,
        directory_structure_enabled: cfg.output.directory_structure,
        files_enabled: cfg.output.files,
        escape_file_content: cfg.output.parsable_style,
        markdown_code_block_delimiter: delimiter,
        git_diff_enabled: cfg.output.git.include_diffs,
        git_diff_work_tree: git_diff_result.work_tree_diff_content.clone(),
        git_diff_staged: git_diff_result.staged_diff_content.clone(),
        git_log_enabled: cfg.output.git.include_logs,
        git_log_commits: git_log_result.commits.clone(),
    }
}

/// Load the instruction file, if configured.
fn load_instruction(cfg: &Config) -> Result<Option<String>> {
    let Some(path) = &cfg.output.instruction_file_path else {
        return Ok(None);
    };
    let resolved = resolve_path(&cfg.cwd.join(path));
    let content = std::fs::read_to_string(&resolved).map_err(|_| {
        anyhow::anyhow!("Instruction file not found at {}", resolved.display())
    })?;
    Ok(Some(content))
}

/// Group files by their first path segment (used by `--split-output`).
pub fn group_by_root_entry(paths: &[String]) -> Vec<(String, Vec<String>)> {
    let mut groups: HashMap<String, Vec<String>> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for path in paths {
        let entry = path
            .split('/')
            .next()
            .unwrap_or(path)
            .to_string();
        if !groups.contains_key(&entry) {
            order.push(entry.clone());
        }
        groups.entry(entry).or_default().push(path.clone());
    }
    order.sort_by(|a, b| crate::util::path::locale_cmp(a, b));
    order.into_iter()
        .map(|k| {
            let mut v = groups.remove(&k).unwrap();
            sort_paths(&mut v);
            (k, v)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_display_paths() {
        assert_eq!(join_display_path("src", "a/b.rs"), "src/a/b.rs");
        assert_eq!(join_display_path("/src/", "/a/b.rs"), "src/a/b.rs");
        assert_eq!(join_display_path("src", ""), "src");
    }

    #[test]
    fn computes_relative_paths() {
        assert_eq!(
            path_relative(Path::new("/a/b"), Path::new("/a/b/c/d")),
            "c/d"
        );
        assert_eq!(path_relative(Path::new("/a/b"), Path::new("/a/x")), "../x");
    }

    #[test]
    fn extracts_extensions() {
        assert_eq!(extension_of("a/b/c.RS"), "rs");
        assert_eq!(extension_of("Makefile"), "");
    }
}
