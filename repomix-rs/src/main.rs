//! Rustmix - an ultra-fast codebase packer for frontier reasoning models.

use std::path::PathBuf;

use rustmix::cli::{self, Cli};
use rustmix::config::{self, Config};
use rustmix::core::packager;
use rustmix::output;
use rustmix::report;

use anyhow::{bail, Context, Result};
use clap::Parser;



fn main() {
    let process_start = std::time::Instant::now();
    let timing = std::env::var_os("RUSTMIX_TIMING").or_else(|| std::env::var_os("REPOMIX_TIMING")).is_some();
    let args: Vec<String> = std::env::args().collect();
    let mut normalized: Vec<String> = vec![args[0].clone()];
    normalized.extend(cli::normalize_optional_value_args(&args[1..]));
    let cli = match Cli::try_parse_from(normalized) {
        Ok(cli) => cli,
        Err(err) => {
            // clap prints help/version and exits for those cases.
            let _ = err.print();
            std::process::exit(if err.use_stderr() { 2 } else { 0 });
        }
    };

    match run(cli) {
        Ok(code) => {
            if timing {
                eprintln!(
                    "[timing] {:<22} {:>8.1}ms",
                    "total (in main)",
                    process_start.elapsed().as_secs_f64() * 1000.0
                );
            }
            std::process::exit(code)
        }
        Err(err) => {
            eprintln!("\u{2716} {err}");
            std::process::exit(1);
        }
    }
}

/// Read the file paths from stdin (the `--stdin` mode).
///
/// The lines are trimmed, empty lines and comments (starting with `#`) are
/// dropped, and the remaining paths are resolved against the current
/// directory and de-duplicated.
fn read_stdin_paths(cwd: &std::path::Path) -> Result<Vec<PathBuf>> {
    use std::io::{BufRead, IsTerminal};

    if std::io::stdin().is_terminal() {
        bail!("No data provided via stdin. Please pipe file paths to rustmix when using --stdin flag.");
    }

    let stdin = std::io::stdin();
    let mut lines: Vec<String> = Vec::new();
    for line in stdin.lock().lines() {
        lines.push(line?);
    }

    let valid: Vec<&str> = lines
        .iter()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();
    if valid.is_empty() {
        bail!("No valid file paths found in stdin input.");
    }

    let mut seen: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
    let mut out: Vec<PathBuf> = Vec::new();
    for line in valid {
        let path = std::path::Path::new(line);
        let resolved = if path.is_absolute() {
            path.to_path_buf()
        } else {
            cwd.join(path)
        };
        // Normalize the path, so that the same file is not packed twice.
        let normalized = packager::resolve_path(&resolved);
        if seen.insert(normalized.clone()) {
            out.push(normalized);
        }
    }
    Ok(out)
}

fn self_update() -> Result<i32> {
    println!("\u{2728} Updating Rustmix to the latest release...");

    #[cfg(windows)]
    {
        let status = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                "irm https://raw.githubusercontent.com/ollezetterstrom/Rustmix/main/install.ps1 | iex",
            ])
            .status()
            .context("Failed to run PowerShell update command")?;
        if status.success() {
            println!("\u{2705} Rustmix update completed!");
            Ok(0)
        } else {
            bail!("Update failed with exit status: {:?}", status.code());
        }
    }

    #[cfg(not(windows))]
    {
        let status = std::process::Command::new("sh")
            .args([
                "-c",
                "curl -fsSL https://raw.githubusercontent.com/ollezetterstrom/Rustmix/main/install.sh | sh",
            ])
            .status()
            .context("Failed to run shell update command")?;
        if status.success() {
            println!("\u{2705} Rustmix update completed!");
            Ok(0)
        } else {
            bail!("Update failed with exit status: {:?}", status.code());
        }
    }
}

fn run(cli: Cli) -> Result<i32> {
    if cli.update || (cli.directories.len() == 1 && cli.directories[0] == "update") {
        return self_update();
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let timing = std::env::var_os("RUSTMIX_TIMING").or_else(|| std::env::var_os("REPOMIX_TIMING")).is_some();
    let t_run = std::time::Instant::now();

    // Load the configuration file (if any).
    let config_path = config::find_config_file(&cwd, cli.config.as_deref());
    let file_config = match &config_path {
        Some(path) => Some(config::parse_config_file(path)?),
        None => None,
    };
    let cfg = Config::build(cwd.clone(), &cli, file_config.as_ref());

    // The token counting phase is the most expensive part of a run, and its
    // result is only ever read by the console report and the token budget
    // check. Skip it entirely when neither is active.
    let mut cfg = cfg;
    cfg.need_metrics = (!cli.quiet && !cfg.output.stdout) || cfg.output.token_budget.is_some();

    if cli.quiet {
        // Nothing is printed except errors.
    } else if !cfg.output.stdout {
        println!("\n\u{1F4E6} Rustmix v{}\n", env!("CARGO_PKG_VERSION"));
    }

    // Resolve the target directories.
    let root_dirs: Vec<PathBuf> = cli
        .directories
        .iter()
        .map(|d| packager::resolve_path(&cwd.join(d)))
        .collect();
    if root_dirs.is_empty() {
        bail!("No target directory specified");
    }

    // The `--stdin` mode reads the file paths from stdin instead of walking
    // the target directory.
    let mut cfg = cfg;
    if cli.stdin {
        let first_dir = cli.directories.first().map(|s| s.as_str()).unwrap_or(".");
        if cli.directories.len() > 1 || first_dir != "." {
            bail!("When using --stdin, do not specify directory arguments. File paths will be read from stdin.");
        }
        cfg.stdin_file_paths = Some(read_stdin_paths(&cwd)?);
    }

    let mut progress = |message: &str| {
        if !cli.quiet && !cfg.output.stdout && cli.verbose {
            eprintln!("  {message}");
        }
    };

    let result = packager::pack(&root_dirs, &cfg, &mut progress)?;
    if timing {
        eprintln!(
            "[timing] {:<22} {:>8.1}ms",
            "pack (in run)",
            t_run.elapsed().as_secs_f64() * 1000.0
        );
    }

    // --- Write the output ------------------------------------------------
    let mut output_files: Vec<PathBuf> = Vec::new();

    if cfg.output.split_output.is_some() {
        if cfg.output.stdout {
            bail!("--split-output cannot be used with --stdout. Split output requires writing to filesystem.");
        }
        if cfg.output.copy_to_clipboard {
            bail!("--split-output cannot be used with --copy. Split output generates multiple files.");
        }
        let parts = output::writer::generate_split_parts(&cfg, &result)?;
        for part in &parts {
            std::fs::create_dir_all(part.file_path.parent().unwrap_or(&cwd))?;
            std::fs::write(&part.file_path, part.content.as_bytes())?;
            output_files.push(part.file_path.clone());
        }
    } else if !result.output_files.is_empty() || cfg.output.stdout {
        // Streamed directly to file or stdout during packing
        output_files = result.output_files.clone();
    } else {
        let t_write = std::time::Instant::now();
        output_files = output::writer::write_output(&cfg, &result.output)?;
        if timing {
            eprintln!(
                "[timing] {:<22} {:>8.1}ms",
                "write output",
                t_write.elapsed().as_secs_f64() * 1000.0
            );
        }
    }

    if cfg.output.copy_to_clipboard {
        output::writer::copy_to_clipboard(&result.output)?;
    }

    // --- Report ------------------------------------------------------------
    if !cli.quiet && !cfg.output.stdout {
        report::report_top_files(
            &result.metrics.file_metrics,
            cfg.output.top_files_length,
            result.metrics.total_tokens,
        );
        if let Some(threshold) = cfg.output.token_count_tree {
            report::report_token_count_tree(&result.metrics.file_metrics, threshold);
            println!();
        }
        report::report_security_check(&result);
        println!();
        report::report_skipped_files(&result);
        println!();

        let display = if output_files.len() > 1 {
            format!(
                "{} ... {} ({} parts)",
                report::display_path(&output_files[0], &cwd),
                report::display_path(&output_files[output_files.len() - 1], &cwd),
                output_files.len()
            )
        } else if let Some(first) = output_files.first() {
            report::display_path(first, &cwd)
        } else {
            cfg.output.file_path.clone()
        };
        report::report_summary(&cfg, &result, &display);
        report::report_completion();
    }

    // --- Token budget ------------------------------------------------------
    if let Some(budget) = cfg.output.token_budget {
        if result.metrics.total_tokens > budget as usize {
            eprintln!(
                "\u{2716} Token budget exceeded: {} tokens > {} tokens",
                result.metrics.total_tokens, budget
            );
            return Ok(1);
        }
    }

    // The result holds ~90 MB of file content and the ~65 MB output. The
    // process exits right away, so the memory does not have to be freed one
    // block at a time - the operating system reclaims the whole address
    // space at once.
    //
    // The library API is not affected: `pack()` still returns the result and
    // frees it normally.
    if std::env::var_os("RUSTMIX_TIMING_DROP").or_else(|| std::env::var_os("REPOMIX_TIMING_DROP")).is_some() {
        let t_drop = std::time::Instant::now();
        drop(result);
        eprintln!(
            "[timing] {:<22} {:>8.1}ms",
            "teardown (drop)",
            t_drop.elapsed().as_secs_f64() * 1000.0
        );
    } else {
        std::mem::forget(result);
    }

    Ok(0)
}
