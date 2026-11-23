//! Micro-benchmark for the security scan.
//!
//! Separates the time of the literal prefilter (the aho-corasick pass over
//! the whole corpus) from the time of the regular expressions that the
//! prefilter enables, and compares the aho-corasick implementations.
//!
//! Usage: bench_sec <file-or-dir> [...]

use std::time::Instant;

use aho_corasick::{AhoCorasick, AhoCorasickBuilder, MatchKind};
use rayon::prelude::*;

fn collect(paths: &[String]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for p in paths {
        let meta = std::fs::metadata(p).unwrap();
        if meta.is_file() {
            if let Ok(s) = std::fs::read_to_string(p) {
                out.push((p.clone(), s));
            }
            continue;
        }
        let mut stack = vec![std::path::PathBuf::from(p)];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for e in entries.flatten() {
                let path = e.path();
                if path.is_dir() {
                    stack.push(path);
                } else if let Ok(s) = std::fs::read_to_string(&path) {
                    out.push((path.display().to_string(), s));
                }
            }
        }
    }
    out
}

fn scan(ac: &AhoCorasick, files: &[(String, String)]) -> usize {
    let mut hits = 0usize;
    for (_, s) in files {
        for m in ac.find_iter(s) {
            hits += m.pattern().as_usize() + 1;
        }
    }
    hits
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: bench_sec <file-or-dir> [...]");
        std::process::exit(2);
    }
    let files = collect(&args);
    let bytes: usize = files.iter().map(|(_, s)| s.len()).sum();
    println!(
        "files={} bytes={} ({:.1} MB)",
        files.len(),
        bytes,
        bytes as f64 / 1e6
    );

    let literals = rustmix::core::security::rule_literals();
    println!("literals: {}", literals.len());

    // The full scan.
    let t = Instant::now();
    let mut issues = 0usize;
    for (_, s) in &files {
        issues += rustmix::core::security::count_issues(s);
    }
    let full = t.elapsed();
    println!(
        "full scan (1 thread)   : {:>8.1} ms  {:>6.1} MB/s  issues={issues}",
        full.as_secs_f64() * 1000.0,
        bytes as f64 / 1e6 / full.as_secs_f64()
    );

    // The prefilter on its own.
    let t = Instant::now();
    let mut candidates = 0usize;
    for (_, s) in &files {
        candidates += rustmix::core::security::candidate_rule_count(s);
    }
    let pre = t.elapsed();
    println!(
        "prefilter (1 thread)   : {:>8.1} ms  {:>6.1} MB/s  candidate rules={candidates}",
        pre.as_secs_f64() * 1000.0,
        bytes as f64 / 1e6 / pre.as_secs_f64()
    );

    // The aho-corasick implementations on their own.
    for (label, dfa) in [("nfa", false)] {
        let t = Instant::now();
        let ac = AhoCorasickBuilder::new()
            .match_kind(MatchKind::Standard)
            .build(&literals)
            .unwrap();
        let build = t.elapsed();

        let t = Instant::now();
        let hits = scan(&ac, &files);
        let scan_time = t.elapsed();
        println!(
            "aho-corasick {label:<4}      : build={:>7.1} ms  scan={:>8.1} ms  {:>6.1} MB/s  hits={hits}",
            build.as_secs_f64() * 1000.0,
            scan_time.as_secs_f64() * 1000.0,
            bytes as f64 / 1e6 / scan_time.as_secs_f64()
        );
    }

    // The same alternation with the regex crate (a lazy DFA).
    let pattern = literals
        .iter()
        .map(|l| regex::escape(l))
        .collect::<Vec<_>>()
        .join("|");
    let t = Instant::now();
    let re = regex::Regex::new(&format!("(?:{pattern})")).unwrap();
    let build = t.elapsed();
    let t = Instant::now();
    let mut hits = 0usize;
    for (_, s) in &files {
        for m in re.find_iter(s) {
            hits += m.as_str().len();
        }
    }
    let scan_time = t.elapsed();
    println!(
        "regex crate (lazy DFA) : build={:>7.1} ms  scan={:>8.1} ms  {:>6.1} MB/s  hits={hits}",
        build.as_secs_f64() * 1000.0,
        scan_time.as_secs_f64() * 1000.0,
        bytes as f64 / 1e6 / scan_time.as_secs_f64()
    );

    // The same in parallel - the lazy DFA of the regex crate keeps its cache
    // behind a mutex, so this shows whether it scales.
    let t = Instant::now();
    let hits2: usize = files.par_iter().map(|(_, s)| re.find_iter(s).count()).sum();
    let regex_par = t.elapsed();
    println!(
        "regex crate (rayon)    : scan={:>8.1} ms  {:>6.1} MB/s  hits={hits2}  speedup={:.2}x",
        regex_par.as_secs_f64() * 1000.0,
        bytes as f64 / 1e6 / regex_par.as_secs_f64(),
        scan_time.as_secs_f64() / regex_par.as_secs_f64()
    );

    // The same for aho-corasick in parallel.
    let ac = AhoCorasickBuilder::new()
        .match_kind(MatchKind::Standard)
        .build(&literals)
        .unwrap();
    let t = Instant::now();
    let hits3: usize = files.par_iter().map(|(_, s)| ac.find_iter(s).count()).sum();
    let ac_par = t.elapsed();
    println!(
        "aho-corasick (rayon)   : scan={:>8.1} ms  {:>6.1} MB/s  hits={hits3}  speedup={:.2}x",
        ac_par.as_secs_f64() * 1000.0,
        bytes as f64 / 1e6 / ac_par.as_secs_f64(),
        scan_time.as_secs_f64() / ac_par.as_secs_f64()
    );
}

