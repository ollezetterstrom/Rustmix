//! File discovery.
//!
//! Mirrors Repomix's `searchFiles`: a parallel directory walk that honours
//! `.gitignore` / `.ignore` files, the built-in ignore list, user supplied
//! include/ignore globs and the "empty directory" rule.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{bail, Result};
use ignore::WalkBuilder;

use crate::config::Config;
use crate::util::glob::{self, PatternSet};
use crate::util::path::sort_paths;

/// Repomix's built-in ignore list.
pub const DEFAULT_IGNORE_LIST: &[&str] = &[
    ".git/**",
    ".hg/**",
    ".hgignore",
    ".svn/**",
    "**/node_modules/**",
    "**/bower_components/**",
    "**/jspm_packages/**",
    "vendor/**",
    "**/.bundle/**",
    "**/.gradle/**",
    "target/**",
    "logs/**",
    "**/*.log",
    "**/npm-debug.log*",
    "**/yarn-debug.log*",
    "**/yarn-error.log*",
    "pids/**",
    "*.pid",
    "*.seed",
    "*.pid.lock",
    "lib-cov/**",
    "coverage/**",
    ".nyc_output/**",
    ".grunt/**",
    ".lock-wscript",
    "build/Release/**",
    "typings/**",
    "**/.npm/**",
    ".eslintcache",
    ".rollup.cache/**",
    ".webpack.cache/**",
    ".parcel-cache/**",
    ".sass-cache/**",
    "*.cache",
    ".node_repl_history",
    "*.tgz",
    "**/.yarn/**",
    "**/.yarn-integrity",
    ".env",
    ".next/**",
    ".nuxt/**",
    ".vuepress/dist/**",
    ".serverless/**",
    ".fusebox/**",
    ".dynamodb/**",
    "dist/**",
    "**/.DS_Store",
    "**/Thumbs.db",
    ".idea/**",
    ".vscode/**",
    "**/*.swp",
    "**/*.swo",
    "**/*.swn",
    "**/*.bak",
    "build/**",
    "out/**",
    "tmp/**",
    "temp/**",
    "**/repomix-output.*",
    "**/repopack-output.*",
    "**/package-lock.json",
    "**/yarn-error.log",
    "**/yarn.lock",
    "**/pnpm-lock.yaml",
    "**/bun.lockb",
    "**/bun.lock",
    "**/__pycache__/**",
    "**/*.py[cod]",
    "**/venv/**",
    "**/.venv/**",
    "**/.pytest_cache/**",
    "**/.mypy_cache/**",
    "**/.ipynb_checkpoints/**",
    "**/Pipfile.lock",
    "**/poetry.lock",
    "**/uv.lock",
    "**/Cargo.lock",
    "**/Cargo.toml.orig",
    "**/target/**",
    "**/*.rs.bk",
    "**/composer.lock",
    "**/Gemfile.lock",
    "**/go.sum",
    "**/mix.lock",
    "**/stack.yaml.lock",
    "**/cabal.project.freeze",
];

/// File names that hold ignore rules.
const IGNORE_CONTROL_FILE_NAMES: [&str; 3] = [".gitignore", ".ignore", ".repomixignore"];

/// The result of a directory scan.
#[derive(Debug, Default)]
pub struct SearchOutcome {
    /// Root relative, POSIX separated file paths.
    pub file_paths: Vec<String>,
    /// Root relative, POSIX separated empty directory paths.
    pub empty_dir_paths: Vec<String>,
}

/// Compiled matcher used while walking a tree.
pub struct Matchers {
    include: PatternSet,
    ignore: PatternSet,
    /// Ignore patterns that name an ignore-control file; these are matched
    /// against the path itself and against `path/**`.
    deferred: PatternSet,
    deferred_dir: PatternSet,
}

impl Matchers {
    /// Should the file at `rel_path` be excluded by the ignore rules?
    pub fn is_ignored(&self, rel_path: &str) -> bool {
        if self.ignore.is_match(rel_path) {
            return true;
        }
        if self.deferred.is_match(rel_path) {
            return true;
        }
        // A deferred pattern that names a directory also excludes its content.
        if !self.deferred_dir.is_empty() && self.deferred_dir.is_match(rel_path) {
            return true;
        }
        false
    }

    /// Is the file at `rel_path` part of the pack?
    ///
    /// The path is split into its segments once, and the result is used for
    /// all the pattern sets.
    pub fn check_file(&self, rel_path: &str) -> bool {
        let segments = crate::util::path::split_segments(rel_path);
        if self.ignore.is_match_segments(&segments)
            || self.deferred.is_match_segments(&segments)
            || (!self.deferred_dir.is_empty() && self.deferred_dir.is_match_segments(&segments))
        {
            return false;
        }
        self.include.is_match_segments(&segments)
    }

    /// Should the directory at `rel_path` be pruned from the walk?
    pub fn should_prune_dir(&self, rel_path: &str) -> bool {
        self.ignore.prunes_dir(rel_path)
    }

    /// Does the include filter accept the given path?
    pub fn is_included(&self, rel_path: &str) -> bool {
        self.include.is_match(rel_path)
    }
}

/// Build the matchers for a given root directory.
pub fn build_matchers(root: &Path, cfg: &Config) -> Matchers {
    let mut ignore_patterns: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    let push = |patterns: &mut Vec<String>, seen: &mut std::collections::HashSet<String>, p: &str| {
        if seen.insert(p.to_string()) {
            patterns.push(p.to_string());
        }
    };

    if cfg.ignore.use_default_patterns {
        for p in DEFAULT_IGNORE_LIST {
            push(&mut ignore_patterns, &mut seen, p);
        }
    }

    // The output file itself must never be part of the packed output.
    let output_abs = if Path::new(&cfg.output.file_path).is_absolute() {
        PathBuf::from(&cfg.output.file_path)
    } else {
        cfg.cwd.join(&cfg.output.file_path)
    };
    if let Ok(rel) = pathdiff_relative(root, &output_abs) {
        let rel = crate::util::path::to_posix(&rel);
        push(&mut ignore_patterns, &mut seen, &rel);
    }

    for p in &cfg.ignore.custom_patterns {
        push(&mut ignore_patterns, &mut seen, p);
    }

    if cfg.ignore.use_gitignore {
        let exclude = root.join(".git").join("info").join("exclude");
        if let Ok(content) = std::fs::read_to_string(&exclude) {
            for line in content.lines() {
                let trimmed = line.trim();
                if !trimmed.is_empty() && !trimmed.starts_with('#') {
                    push(&mut ignore_patterns, &mut seen, trimmed);
                }
            }
        }
    }

    // Split ignore-control-file patterns out; they are matched separately.
    let mut globby_ignore = PatternSet::new();
    let mut deferred = PatternSet::new();
    let mut deferred_dir = PatternSet::new();
    for pattern in &ignore_patterns {
        if is_ignore_control_file_pattern(pattern) {
            deferred.add(pattern);
            deferred_dir.add(&format!("{pattern}/**"));
        } else {
            globby_ignore.add(&glob::normalize_glob_pattern(pattern));
        }
    }

    let mut include = PatternSet::new();
    if cfg.include.is_empty() {
        include.add("**/*");
    } else {
        for pattern in &cfg.include {
            include.add(&glob::escape_glob_pattern(pattern));
        }
    }

    Matchers {
        include,
        ignore: globby_ignore,
        deferred,
        deferred_dir,
    }
}

fn is_ignore_control_file_pattern(pattern: &str) -> bool {
    if pattern.starts_with('!') {
        return false;
    }
    let normalized = crate::util::path::to_posix(pattern);
    let normalized = normalized.trim_end_matches('/');
    let base = normalized.rsplit('/').next().unwrap_or(normalized);
    IGNORE_CONTROL_FILE_NAMES.contains(&base)
}

/// Compute the relative path from `base` to `target` using POSIX semantics.
fn pathdiff_relative(base: &Path, target: &Path) -> Result<String, ()> {
    let base = base.canonicalize().unwrap_or_else(|_| base.to_path_buf());
    let target = target.canonicalize().unwrap_or_else(|_| target.to_path_buf());
    let base_parts: Vec<_> = base.components().collect();
    let target_parts: Vec<_> = target.components().collect();
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
        parts.push(c.as_os_str().to_string_lossy().to_string());
    }
    if parts.is_empty() {
        return Ok(String::new());
    }
    Ok(parts.join("/"))
}

/// Scan a directory and return the matching files (and empty directories).
pub fn search_files(root: &Path, cfg: &Config) -> Result<SearchOutcome> {
    let metadata = std::fs::metadata(root).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => anyhow::anyhow!("Target path does not exist: {}", root.display()),
        std::io::ErrorKind::PermissionDenied => anyhow::anyhow!(
            "Permission denied while accessing path. Please check folder access permissions for your terminal app. path: {}",
            root.display()
        ),
        _ => anyhow::anyhow!("Failed to access path: {}. Error code: {}", root.display(), e),
    })?;
    if !metadata.is_dir() {
        bail!(
            "Target path is not a directory: {}. Please specify a directory path, not a file path.",
            root.display()
        );
    }

    let matchers = Arc::new(build_matchers(root, cfg));
    let include_empty_directories = cfg.output.include_empty_directories.unwrap_or(false);

    let mut builder = WalkBuilder::new(root);
    builder
        .standard_filters(false)
        .hidden(false)
        .git_ignore(cfg.ignore.use_gitignore)
        .git_global(false)
        .git_exclude(cfg.ignore.use_gitignore)
        .ignore(cfg.ignore.use_dot_ignore)
        .require_git(false)
        .parents(true)
        .follow_links(false)
        .same_file_system(false);
    builder.add_custom_ignore_filename(".repomixignore");

    let matchers_for_filter = Arc::clone(&matchers);
    let filter_root = root.to_path_buf();
    builder.filter_entry(move |entry| {
        if entry.depth() == 0 {
            return true;
        }
        let Ok(rel) = relative_posix(&filter_root, entry.path()) else {
            return false;
        };
        if entry.file_type().is_some_and(|ft| ft.is_dir()) {
            !matchers_for_filter.should_prune_dir(&rel)
        } else {
            true
        }
    });

    let mut file_paths: Vec<String> = Vec::new();
    let mut dir_paths: Vec<String> = Vec::new();

    // Fine grained instrumentation of the search phase.
    let timing = std::env::var_os("REPOMIX_TIMING").is_some();
    let t_walk = std::time::Instant::now();
    let mut t_match = std::time::Duration::ZERO;
    for entry in builder.build() {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let ft = match entry.file_type() {
            Some(ft) => ft,
            None => continue,
        };
        if ft.is_symlink() {
            // Symlinks are not followed nor included.
            continue;
        }
        let Ok(rel) = relative_posix(root, entry.path()) else {
            continue;
        };
        if rel.is_empty() {
            continue;
        }
        if ft.is_dir() {
            if include_empty_directories {
                dir_paths.push(rel);
            }
            continue;
        }
        if !ft.is_file() {
            continue;
        }
        let t0 = std::time::Instant::now();
        let included = matchers.check_file(&rel);
        t_match += t0.elapsed();
        if !included {
            continue;
        }
        file_paths.push(rel);
    }
    let t_walk = t_walk.elapsed();

    let t_sort = std::time::Instant::now();
    sort_paths(&mut file_paths);
    let t_sort = t_sort.elapsed();

    if timing {
        eprintln!(
            "[timing]   walk={t_walk:?} match={t_match:?} sort={t_sort:?} files={}",
            file_paths.len()
        );
    }

    let mut empty_dir_paths = Vec::new();
    if include_empty_directories {
        for dir in &dir_paths {
            if is_empty_directory(root, dir) {
                empty_dir_paths.push(dir.clone());
            }
        }
        sort_paths(&mut empty_dir_paths);
    }

    Ok(SearchOutcome {
        file_paths,
        empty_dir_paths,
    })
}

/// A directory is considered empty when it holds no entries that do not start
/// with a dot.
fn is_empty_directory(root: &Path, rel: &str) -> bool {
    let full = root.join(rel);
    match std::fs::read_dir(&full) {
        Ok(entries) => !entries.flatten().any(|e| {
            !e.file_name()
                .to_string_lossy()
                .starts_with('.')
        }),
        Err(_) => false,
    }
}

/// Collect an explicit list of files instead of walking the tree.
///
/// This is what the `--stdin` mode does: the file paths read from stdin
/// replace the directory walk. The normal ignore rules still apply, and
/// without a user supplied `--include` pattern only the listed files are
/// packed.
pub fn search_explicit_files(
    root: &Path,
    cfg: &Config,
    explicit_files: &[PathBuf],
) -> Result<SearchOutcome> {
    let matchers = build_matchers(root, cfg);
    let user_has_include = !cfg.include.is_empty();
    let mut file_paths: Vec<String> = Vec::with_capacity(explicit_files.len());

    for file in explicit_files {
        // A directory is expanded into the files below it, like the
        // reference implementation does.
        if file.is_dir() {
            let Ok(rel_dir) = relative_posix(root, file) else {
                continue;
            };
            if let Ok(found) = search_files(file, cfg) {
                for rel in found.file_paths {
                    let joined = if rel_dir.is_empty() {
                        rel
                    } else {
                        format!("{rel_dir}/{rel}")
                    };
                    if !matchers.is_ignored(&joined) {
                        file_paths.push(joined);
                    }
                }
            }
            continue;
        }

        let Ok(rel) = relative_posix(root, file) else {
            continue;
        };
        // A file outside of the root directory is not part of the pack.
        if rel.is_empty() || rel.starts_with("..") {
            continue;
        }
        if matchers.is_ignored(&rel) {
            continue;
        }
        // The listed files are always included; a user supplied include
        // pattern adds to them (the reference implementation merges the two
        // lists of patterns).
        if !user_has_include && !matchers.is_included(&rel) {
            continue;
        }
        file_paths.push(rel);
    }

    file_paths.sort();
    file_paths.dedup();
    sort_paths(&mut file_paths);

    Ok(SearchOutcome {
        file_paths,
        empty_dir_paths: Vec::new(),
    })
}

/// Compute a root relative, POSIX separated path.
fn relative_posix(root: &Path, path: &Path) -> Result<String, ()> {
    let rel = path.strip_prefix(root).map_err(|_| ())?;
    let mut out = String::with_capacity(rel.as_os_str().len());
    let mut first = true;
    for comp in rel.components() {
        let s = comp.as_os_str().to_string_lossy();
        if s == "." {
            continue;
        }
        if !first {
            out.push('/');
        }
        out.push_str(&s);
        first = false;
    }
    Ok(out)
}

/// List every directory below `root`, honouring the ignore rules.
pub fn list_directories(root: &Path, cfg: &Config) -> Result<Vec<String>> {
    let matchers = build_matchers(root, cfg);
    let mut builder = WalkBuilder::new(root);
    builder
        .standard_filters(false)
        .hidden(false)
        .git_ignore(cfg.ignore.use_gitignore)
        .git_global(false)
        .git_exclude(cfg.ignore.use_gitignore)
        .ignore(cfg.ignore.use_dot_ignore)
        .require_git(false)
        .parents(true)
        .follow_links(false);
    builder.add_custom_ignore_filename(".repomixignore");
    let matchers = Arc::new(matchers);
    let m = Arc::clone(&matchers);
    let filter_root = root.to_path_buf();
    builder.filter_entry(move |entry| {
        if entry.depth() == 0 {
            return true;
        }
        let Ok(rel) = relative_posix(&filter_root, entry.path()) else {
            return false;
        };
        if entry.file_type().is_some_and(|ft| ft.is_dir()) {
            !m.should_prune_dir(&rel)
        } else {
            true
        }
    });

    let mut dirs = Vec::new();
    for entry in builder.build().flatten() {
        let Some(ft) = entry.file_type() else { continue };
        if !ft.is_dir() || ft.is_symlink() {
            continue;
        }
        let Ok(rel) = relative_posix(root, entry.path()) else {
            continue;
        };
        if rel.is_empty() {
            continue;
        }
        if matchers.is_ignored(&rel) {
            continue;
        }
        dirs.push(rel);
    }
    sort_paths(&mut dirs);
    Ok(dirs)
}

/// List every file below `root`, honouring the ignore rules.
pub fn list_files(root: &Path, cfg: &Config) -> Result<Vec<String>> {
    let matchers = build_matchers(root, cfg);
    let mut builder = WalkBuilder::new(root);
    builder
        .standard_filters(false)
        .hidden(false)
        .git_ignore(cfg.ignore.use_gitignore)
        .git_global(false)
        .git_exclude(cfg.ignore.use_gitignore)
        .ignore(cfg.ignore.use_dot_ignore)
        .require_git(false)
        .parents(true)
        .follow_links(false);
    builder.add_custom_ignore_filename(".repomixignore");
    let matchers = Arc::new(matchers);
    let m = Arc::clone(&matchers);
    let filter_root = root.to_path_buf();
    builder.filter_entry(move |entry| {
        if entry.depth() == 0 {
            return true;
        }
        let Ok(rel) = relative_posix(&filter_root, entry.path()) else {
            return false;
        };
        if entry.file_type().is_some_and(|ft| ft.is_dir()) {
            !m.should_prune_dir(&rel)
        } else {
            true
        }
    });

    let mut files = Vec::new();
    for entry in builder.build().flatten() {
        let Some(ft) = entry.file_type() else { continue };
        if !ft.is_file() || ft.is_symlink() {
            continue;
        }
        let Ok(rel) = relative_posix(root, entry.path()) else {
            continue;
        };
        if rel.is_empty() {
            continue;
        }
        if matchers.is_ignored(&rel) {
            continue;
        }
        files.push(rel);
    }
    sort_paths(&mut files);
    Ok(files)
}
