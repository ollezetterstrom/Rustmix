//! Git integration: diffs, logs and change-frequency sorting.

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

/// Result of collecting the working tree and staged diffs.
#[derive(Debug, Default, Clone)]
pub struct GitDiffResult {
    pub work_tree_diff_content: String,
    pub staged_diff_content: String,
}

/// A single commit in the log output.
#[derive(Debug, Default, Clone)]
pub struct GitLogCommit {
    pub date: String,
    pub message: String,
    pub files: Vec<String>,
}

/// Result of collecting the commit history.
#[derive(Debug, Default, Clone)]
pub struct GitLogResult {
    pub log_content: String,
    pub commits: Vec<GitLogCommit>,
}

/// Arguments that prevent git from executing repository-local hooks or
/// external diff drivers.
const UNTRUSTED_CONFIG_ARGS: &[&str] = &["-c", "core.fsmonitor="];
const LOG_HARDENING_ARGS: &[&str] = &["--no-show-signature", "--no-ext-diff", "--no-textconv"];
const DIFF_HARDENING_ARGS: &[&str] = &["--no-ext-diff", "--no-textconv"];

/// Run a git command inside `dir` with the hardened arguments.
fn run_git(dir: &Path, args: &[&str]) -> Option<String> {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(dir);
    for a in UNTRUSTED_CONFIG_ARGS {
        cmd.arg(a);
    }
    for a in args {
        cmd.arg(a);
    }
    cmd.env("GIT_TERMINAL_PROMPT", "0");
    cmd.env("GIT_PAGER", "cat");
    cmd.env("GIT_OPTIONAL_LOCKS", "0");

    let output = cmd.output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Is `dir` inside a git work tree?
pub fn is_git_repository(dir: &Path) -> bool {
    match run_git(dir, &["rev-parse", "--is-inside-work-tree"]) {
        Some(out) => out.trim() == "true",
        None => false,
    }
}

/// Does `dir` contain a `.git` entry?
///
/// The reference implementation only uses the change counts when the
/// *working directory* itself is the root of a work tree - it does not look
/// at the parent directories. `git log` on the other hand finds the parent
/// repository, so the two have to be distinguished.
pub fn is_work_tree_root(dir: &Path) -> bool {
    dir.join(".git").exists()
}

/// Collect the working tree and staged diffs.
pub fn get_diffs(dir: &Path) -> GitDiffResult {
    // No `rev-parse` probe: `git diff` fails in the same way outside a work
    // tree, and every extra process spawn costs several milliseconds on
    // Windows.
    let work_tree = run_git(
        dir,
        &[
            "diff",
            "--no-color",
            "--no-ext-diff",
            "--no-textconv",
        ],
    )
    .unwrap_or_default();
    let staged = run_git(
        dir,
        &[
            "diff",
            "--no-color",
            "--cached",
            "--no-ext-diff",
            "--no-textconv",
        ],
    )
    .unwrap_or_default();
    GitDiffResult {
        work_tree_diff_content: work_tree,
        staged_diff_content: staged,
    }
}

/// Collect the commit history.
pub fn get_logs(dir: &Path, max_commits: usize) -> GitLogResult {
    let count = max_commits.to_string();
    let mut args: Vec<&str> = vec!["log"];
    args.extend_from_slice(LOG_HARDENING_ARGS);
    args.extend_from_slice(&[
        "--pretty=format:%x00%ad|%s",
        "--date=iso",
        "--name-only",
        "-n",
        &count,
    ]);
    let raw = match run_git(dir, &args) {
        Some(out) => out,
        None => return GitLogResult::default(),
    };
    parse_git_log(&raw)
}

/// Parse the raw output of `git log --pretty=format:%x00%ad|%s --name-only`.
pub fn parse_git_log(raw: &str) -> GitLogResult {
    let mut commits = Vec::new();
    for record in raw.split('\u{0}') {
        if record.trim().is_empty() {
            continue;
        }
        let mut lines = Vec::new();
        for line in record.split(['\r', '\n']) {
            if !line.trim().is_empty() {
                lines.push(line.to_string());
            }
        }
        if lines.is_empty() {
            continue;
        }
        let head = &lines[0];
        let (date, message) = match head.find('|') {
            Some(idx) => (head[..idx].to_string(), head[idx + 1..].to_string()),
            None => (String::new(), head.clone()),
        };
        let files = lines[1..].to_vec();
        commits.push(GitLogCommit {
            date,
            message,
            files,
        });
    }
    let log_content = commits
        .iter()
        .map(|c| {
            let mut s = format!("{}\n{}\n", c.date, c.message);
            for f in &c.files {
                s.push_str(f);
                s.push('\n');
            }
            s
        })
        .collect::<Vec<_>>()
        .join("\n");
    GitLogResult {
        log_content,
        commits,
    }
}

/// Count how often each file was changed in the recent history.
pub fn get_change_counts(dir: &Path, max_commits: usize) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    let count = max_commits.to_string();
    let mut args: Vec<&str> = vec!["log"];
    args.extend_from_slice(LOG_HARDENING_ARGS);
    args.extend_from_slice(&["--pretty=format:", "--name-only", "-n", &count]);
    let raw = match run_git(dir, &args) {
        Some(out) => out,
        None => return counts,
    };
    for filename in raw.lines().filter(|l| !l.is_empty()) {
        *counts.entry(filename.to_string()).or_insert(0) += 1;
    }
    counts
}
