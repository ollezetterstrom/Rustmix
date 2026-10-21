//! Repomix - a high performance, drop-in compatible codebase packer.

mod cli;
mod config;
mod core;
mod output;
mod report;
mod util;

use std::path::PathBuf;

use anyhow::{bail, Result};
use clap::Parser;

use cli::Cli;
use config::Config;
use core::packager;

fn main() {
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
        Ok(code) => std::process::exit(code),
        Err(err) => {
            eprintln!("✖ {err}");
            std::process::exit(1);
        }
    }
}

fn run(cli: Cli) -> Result<i32> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    // Load the configuration file (if any).
    let config_path = config::find_config_file(&cwd, cli.config.as_deref());
    let file_config = match &config_path {
        Some(path) => Some(config::parse_config_file(path)?),
        None => None,
    };
    let cfg = Config::build(cwd.clone(), &cli, file_config.as_ref());

    if cli.quiet {
        // Nothing is printed except errors.
    } else if !cfg.output.stdout {
        println!("\n📦 Repomix v{}\n", env!("CARGO_PKG_VERSION"));
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

    let mut progress = |message: &str| {
        if !cli.quiet && !cfg.output.stdout && cli.verbose {
            eprintln!("  {message}");
        }
    };

    let result = packager::pack(&root_dirs, &cfg, &mut progress)?;

    // --- Write the output ------------------------------------------------
    let mut output_files: Vec<PathBuf> = Vec::new();
    let mut final_output = result.output.clone();

    if let Some(max_bytes) = cfg.output.split_output {
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
    } else {
        output_files = output::writer::write_output(&cfg, &result.output)?;
    }

    if cfg.output.copy_to_clipboard {
        output::writer::copy_to_clipboard(&final_output)?;
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
                "✖ Token budget exceeded: {} tokens > {} tokens",
                result.metrics.total_tokens, budget
            );
            return Ok(1);
        }
    }

    Ok(0)
}
