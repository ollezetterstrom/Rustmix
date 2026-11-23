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
use crate::core::metrics::Metrics;
use crate::core::read::{self, SkipReason};
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

/// A writer that tracks bytes and characters on the fly.
pub struct CountingWriter<W> {
    pub inner: W,
    pub bytes: usize,
    pub chars: usize,
}

impl<W: std::io::Write> CountingWriter<W> {
    pub fn new(inner: W) -> Self {
        Self { inner, bytes: 0, chars: 0 }
    }
}

impl<W: std::io::Write> std::io::Write for CountingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.bytes += n;
        self.chars += buf[..n].iter().filter(|&&b| (b as i8) >= -0x40).count();
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// Pack the given root directories.
pub fn pack(
    root_dirs: &[PathBuf],
    cfg: &Config,
    progress: &mut dyn FnMut(&str),
) -> Result<PackResult> {
    let timing = std::env::var_os("REPOMIX_TIMING").is_some();
    let t0 = std::time::Instant::now();

    // The tokenizer tables are built lazily; building them takes ~85 ms.
    // When exact token counting is requested, warm it up in the background.
    if cfg.need_metrics && cfg.exact_tokens {
        crate::core::metrics::warm_up(cfg.token_encoding);
    }
    macro_rules! phase {
        ($name:expr) => {
            if timing {
                eprintln!("[timing] {:<22} {:>8.1}ms", $name, t0.elapsed().as_secs_f64() * 1000.0);
            }
        };
    }

    progress("Searching for files...");

    let sort_by_changes = cfg.output.git.sort_by_changes && git::is_work_tree_root(&cfg.cwd);
    let need_change_counts = sort_by_changes || cfg.output.smart_compress.is_some();
    let change_counts_handle = if need_change_counts {
        let dir = cfg.cwd.clone();
        let max_commits = cfg.output.git.sort_by_changes_max_commits;
        Some(std::thread::spawn(move || {
            git::get_change_counts(&dir, max_commits)
        }))
    } else {
        None
    };

    let search_results: Vec<search::SearchOutcome> = if let Some(files) = &cfg.stdin_file_paths {
        // The `--stdin` mode: the file paths read from stdin replace the
        // directory walk. The root is the current directory.
        let root = root_dirs.first().cloned().unwrap_or_else(|| PathBuf::from("."));
        vec![search::search_explicit_files(&root, cfg, files)?]
    } else {
        root_dirs
            .par_iter()
            .map(|root| search::search_files(root, cfg))
            .collect::<Result<Vec<_>>>()?
    };

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

    phase!("search");
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

    let (change_counts, top_changed_files): (HashMap<String, usize>, std::collections::HashSet<String>) =
        if need_change_counts {
            let counts = change_counts_handle
                .and_then(|handle| handle.join().ok())
                .unwrap_or_default();
            let top_files = if let Some(top_n) = cfg.output.smart_compress {
                let mut entries: Vec<(&String, &usize)> = counts.iter().collect();
                entries.sort_by(|a, b| b.1.cmp(a.1));
                entries.into_iter().take(top_n).map(|(k, _)| k.clone()).collect()
            } else {
                std::collections::HashSet::new()
            };
            (counts, top_files)
        } else {
            (HashMap::new(), std::collections::HashSet::new())
        };

    // --- Fused Pipeline: Read, Security Scan, and Content Processing in one pass ---
    phase!("setup");
    progress("Collecting and processing files...");

    let mut file_entries: Vec<(PathBuf, String)> = Vec::new();
    for root_files in &root_files {
        let root = &root_files.root_dir;
        for (rel, display) in root_files.file_paths.iter().zip(root_files.display_paths.iter()) {
            file_entries.push((root.join(rel), display.clone()));
        }
    }

    enum FusedResult {
        Processed(ProcessedFile),
        Skipped(SkippedFile),
        Suspicious(SecurityResult),
        None,
    }

    let outcomes: Vec<FusedResult> = file_entries
        .into_par_iter()
        .map(|(full_path, display_path)| {
            let raw = read::read_raw_file(&full_path, &display_path, cfg.max_file_size);
            match (raw.content, raw.skipped) {
                (Some(content), _) => {
                    if cfg.security_check_enabled {
                        if let Some(sec) = security::scan_file(&display_path, &content) {
                            return FusedResult::Suspicious(sec);
                        }
                    }
                    let should_compress = if cfg.output.smart_compress.is_some() {
                        !top_changed_files.contains(&display_path)
                    } else {
                        cfg.output.compress
                    };
                    let processed_content = process_content_internal(&display_path, content, cfg, should_compress);
                    FusedResult::Processed(ProcessedFile {
                        path: display_path,
                        content: processed_content,
                    })
                }
                (None, Some(reason)) => FusedResult::Skipped(SkippedFile {
                    path: display_path,
                    reason,
                }),
                (None, None) => FusedResult::None,
            }
        })
        .collect();

    let mut processed: Vec<ProcessedFile> = Vec::with_capacity(outcomes.len());
    let mut skipped_files: Vec<SkippedFile> = Vec::new();
    let mut suspicious_files: Vec<SecurityResult> = Vec::new();

    for outcome in outcomes {
        match outcome {
            FusedResult::Processed(p) => processed.push(p),
            FusedResult::Skipped(s) => skipped_files.push(s),
            FusedResult::Suspicious(sec) => suspicious_files.push(sec),
            FusedResult::None => {}
        }
    }

    phase!("fused_pipeline");

    // --- Sort by git change frequency ------------------------------------
    if sort_by_changes {
        processed.sort_by_key(|f| change_counts.get(&f.path).copied().unwrap_or(0));
    }

    // --- Build the directory tree -----------------------------------------
    let tree_string = build_tree_string(&root_files, &root_labels, &all_file_paths, &empty_dir_paths, cfg);

    phase!("sort");
    // --- Render & Output Streaming -----------------------------------------
    progress("Generating output...");
    let instruction = load_instruction(cfg)?;
    let ctx = build_render_context(cfg, &processed, &tree_string, &instruction, &git_diff_result, &git_log_result);

    let can_stream = cfg.can_stream();
    let mut output = String::new();
    let mut output_files = Vec::new();

    let (total_characters, total_bytes) = if can_stream {
        use std::io::Write;
        if cfg.output.stdout {
            let stdout = std::io::stdout();
            let mut writer = CountingWriter::new(std::io::BufWriter::with_capacity(128 * 1024, stdout.lock()));
            output::render_to_writer(cfg.output.style, &ctx, &mut writer)?;
            writer.flush()?;
            (writer.chars, writer.bytes)
        } else {
            let path = resolve_path(&cfg.cwd.join(&cfg.output.file_path));
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let file = std::fs::File::create(&path)?;
            let mut writer = CountingWriter::new(std::io::BufWriter::with_capacity(128 * 1024, file));
            output::render_to_writer(cfg.output.style, &ctx, &mut writer)?;
            writer.flush()?;
            output_files.push(path);
            (writer.chars, writer.bytes)
        }
    } else {
        output = output::render(cfg.output.style, &ctx);
        (output.chars().count(), output.len())
    };

    phase!("render");
    // --- Metrics ------------------------------------------------------------
    let metrics = if cfg.need_metrics {
        progress("Calculating metrics...");
        if !cfg.exact_tokens {
            let file_metrics = crate::core::metrics::calculate_file_metrics(
                &processed,
                cfg.token_encoding,
                false,
            );
            let total_tokens = crate::core::metrics::estimate_tokens(total_bytes);
            let git_diff_tokens = crate::core::metrics::estimate_tokens(
                git_diff_result.work_tree_diff_content.len() + git_diff_result.staged_diff_content.len(),
            );
            let git_log_tokens = crate::core::metrics::estimate_tokens(git_log_result.log_content.len());

            Metrics {
                total_files: processed.len(),
                total_characters,
                total_tokens,
                file_metrics,
                git_diff_tokens,
                git_log_tokens,
            }
        } else {
            let fast_path = crate::core::metrics::can_use_fast_output_token_path(cfg);
            let (file_metrics, (total_chars_exact, output_tokens)) = rayon::join(
                || crate::core::metrics::calculate_file_metrics(&processed, cfg.token_encoding, true),
                || {
                    let total_chars = total_characters;
                    let tokens = if fast_path {
                        match crate::core::metrics::extract_output_wrapper(&output, &processed) {
                            Some(wrapper) => {
                                crate::core::metrics::count_tokens(&wrapper, cfg.token_encoding)
                            }
                            None => {
                                crate::core::metrics::count_tokens_chunked(&output, cfg.token_encoding)
                            }
                        }
                    } else {
                        crate::core::metrics::count_tokens_chunked(&output, cfg.token_encoding)
                    };
                    (total_chars, tokens)
                },
            );

            let total_tokens = if fast_path {
                file_metrics.iter().map(|m| m.tokens).sum::<usize>() + output_tokens
            } else {
                output_tokens
            };

            let git_diff_tokens = crate::core::metrics::count_tokens_chunked(
                &format!(
                    "{}{}",
                    git_diff_result.work_tree_diff_content, git_diff_result.staged_diff_content
                ),
                cfg.token_encoding,
            );
            let git_log_tokens =
                crate::core::metrics::count_tokens_chunked(&git_log_result.log_content, cfg.token_encoding);

            Metrics {
                total_files: processed.len(),
                total_characters: total_chars_exact,
                total_tokens,
                file_metrics,
                git_diff_tokens,
                git_log_tokens,
            }
        }
    } else {
        Metrics {
            total_files: processed.len(),
            ..Metrics::default()
        }
    };

    phase!("metrics");
    let git_diff_tokens = metrics.git_diff_tokens;
    let git_log_tokens = metrics.git_log_tokens;
    Ok(PackResult {
        output,
        output_files,
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
pub fn process_content(path: &str, content: String, cfg: &Config) -> String {
    process_content_internal(path, content, cfg, cfg.output.compress)
}

pub fn process_content_internal(path: &str, mut content: String, cfg: &Config, should_compress: bool) -> String {
    let ext = extension_of(path);

    // 1. Remove comments.
    if cfg.output.remove_comments {
        if let Some(stripped) = comments::remove_comments(&content, &ext) {
            content = stripped;
        }
    }

    // 2. Compress.
    if should_compress {
        if let Some(compressed) = compress::compress_file(path, &content) {
            content = compressed;
        }
    }

    // 3. Truncate base64, SVG paths, and lines > 500 chars.
    if cfg.output.truncate_base64 {
        content = crate::core::base64::truncate_base64_content(&content);
    }

    // 4. Remove empty lines (only for supported file types).
    if cfg.output.remove_empty_lines && comments::supports_comments(&ext) {
        content = comments::remove_empty_lines(&content);
    }

    // 5. Convert 4-space leading indentation to tabs.
    if cfg.output.tab_indent {
        content = crate::core::indent::convert_indent_to_tabs(&content);
    }

    // 6. Trim.
    output::style_utils::js_trim_in_place(&mut content);

    // 7. Add line numbers.
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
pub fn build_render_context<'a>(
    cfg: &Config,
    processed_files: &'a [ProcessedFile],
    tree_string: &str,
    instruction: &Option<String>,
    git_diff_result: &GitDiffResult,
    git_log_result: &GitLogResult,
) -> RenderContext<'a> {
    let has_instruction = instruction.is_some();
    // The code fence delimiter is only used by the markdown style, and
    // computing it means scanning the whole repository content.
    let delimiter = if cfg.output.style == crate::cli::OutputStyle::Markdown {
        let mut contents: Vec<&str> = processed_files.iter().map(|f| f.content.as_str()).collect();
        contents.push(tree_string);
        contents.push(&git_diff_result.work_tree_diff_content);
        contents.push(&git_diff_result.staged_diff_content);
        output::calculate_markdown_delimiter(&contents)
    } else {
        String::new()
    };

    RenderContext {
        generation_header: decorations::generate_header(cfg),
        summary_purpose: decorations::generate_summary_purpose(cfg),
        summary_file_format: decorations::generate_summary_file_format(),
        summary_usage_guidelines: decorations::generate_summary_usage_guidelines(cfg, has_instruction),
        summary_notes: decorations::generate_summary_notes(cfg),
        header_text: cfg.output.header_text.clone(),
        instruction: instruction.clone(),
        tree_string: tree_string.to_string(),
        processed_files,
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
