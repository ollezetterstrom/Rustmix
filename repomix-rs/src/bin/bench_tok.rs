//! Micro-benchmark for the token counting hot path.
//!
//! Splits the time of `encode_ordinary` into the regex pre-tokenisation and
//! the BPE part, and compares the different scheduling strategies.
//!
//! Usage: bench_tok <file-or-dir> [more...]

use std::time::Instant;

use rayon::prelude::*;

const O200K_PAT_STR: &str = concat!(
    r#"[^\r\n\p{L}\p{N}]?[\p{Lu}\p{Lt}\p{Lm}\p{Lo}\p{M}]*[\p{Ll}\p{Lm}\p{Lo}\p{M}]+(?i:'s|'t|'re|'ve|'m|'ll|'d)?"#,
    "|",
    r#"[^\r\n\p{L}\p{N}]?[\p{Lu}\p{Lt}\p{Lm}\p{Lo}\p{M}]+[\p{Ll}\p{Lm}\p{Lo}\p{M}]*(?i:'s|'t|'re|'ve|'m|'ll|'d)?"#,
    "|",
    r#"\p{N}{1,3}"#,
    "|",
    r#" ?[^\s\p{L}\p{N}]+[\r\n/]*"#,
    "|",
    r#"\s*[\r\n]+"#,
    "|",
    r#"\s+(?!\S)"#,
    "|",
    r#"\s+"#
);

const REPS: usize = 3;

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

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: bench_tok <file-or-dir> [...]");
        std::process::exit(2);
    }

    let files = collect(&args);
    let total_bytes: usize = files.iter().map(|(_, s)| s.len()).sum();
    println!(
        "files={} bytes={} ({:.1} MB)",
        files.len(),
        total_bytes,
        total_bytes as f64 / 1e6
    );

    let bpe = tiktoken_rs::o200k_base_singleton();
    thread_local! {
        static TL_RE: fancy_regex::Regex = fancy_regex::Regex::new(O200K_PAT_STR).unwrap();
    }

    let mut by_size: Vec<&(String, String)> = files.iter().collect();
    by_size.sort_unstable_by(|a, b| b.1.len().cmp(&a.1.len()));

    // The token count has to be identical for every variant - that is the
    // whole point.
    let reference: usize = files
        .par_iter()
        .map(|(_, s)| bpe.encode_ordinary(s).len())
        .sum();

    // 1. rayon, default chunking.
    let mut t_par = Vec::new();
    let mut sum = 0usize;
    for _ in 0..REPS {
        let t = Instant::now();
        sum = files
            .par_iter()
            .map(|(_, s)| bpe.encode_ordinary(s).len())
            .sum();
        t_par.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let m_par = median(t_par);
    println!(
        "encode_ordinary (rayon)      : {m_par:>8.1} ms  {:>6.1} MB/s  tokens={sum}",
        total_bytes as f64 / 1e6 / (m_par / 1000.0)
    );
    assert_eq!(sum, reference);

    // 2. rayon, LPT order (largest first) with one item per task.
    let mut t_lpt = Vec::new();
    let mut sum2 = 0usize;
    for _ in 0..REPS {
        let t = Instant::now();
        sum2 = by_size
            .par_iter()
            .with_max_len(1)
            .map(|(_, s)| bpe.encode_ordinary(s).len())
            .sum();
        t_lpt.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let m_lpt = median(t_lpt);
    println!(
        "encode_ordinary (LPT, len 1)  : {m_lpt:>8.1} ms  {:>6.1} MB/s  tokens={sum2}  speedup={:.2}x",
        total_bytes as f64 / 1e6 / (m_lpt / 1000.0),
        m_par / m_lpt
    );
    assert_eq!(sum2, reference);

    // 3. rayon, LPT order, with a chunk size of 8.
    let mut t_lpt8 = Vec::new();
    let mut sum3 = 0usize;
    for _ in 0..REPS {
        let t = Instant::now();
        sum3 = by_size
            .par_iter()
            .with_max_len(8)
            .map(|(_, s)| bpe.encode_ordinary(s).len())
            .sum();
        t_lpt8.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let m_lpt8 = median(t_lpt8);
    println!(
        "encode_ordinary (LPT, len 8)  : {m_lpt8:>8.1} ms  {:>6.1} MB/s  tokens={sum3}  speedup={:.2}x",
        total_bytes as f64 / 1e6 / (m_lpt8 / 1000.0),
        m_par / m_lpt8
    );
    assert_eq!(sum3, reference);

    // 4. The regex pre-tokenisation on its own (single threaded).
    let mut t_re = Vec::new();
    for _ in 0..REPS {
        let t = Instant::now();
        let mut pieces = 0usize;
        for (_, s) in &files {
            TL_RE.with(|re| {
                for m in re.find_iter(s) {
                    pieces += m.unwrap().as_str().len();
                }
            });
        }
        t_re.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let m_re = median(t_re);
    println!(
        "regex split (1 thread)       : {m_re:>8.1} ms  {:>6.1} MB/s",
        total_bytes as f64 / 1e6 / (m_re / 1000.0)
    );

    // 5. The regex pre-tokenisation in parallel.
    let mut t_re_par = Vec::new();
    for _ in 0..REPS {
        let t = Instant::now();
        let pieces: usize = files
            .par_iter()
            .map(|(_, s)| {
                TL_RE.with(|re| {
                    let mut n = 0usize;
                    for m in re.find_iter(s) {
                        n += m.unwrap().as_str().len();
                    }
                    n
                })
            })
            .sum();
        t_re_par.push(t.elapsed().as_secs_f64() * 1000.0);
        std::hint::black_box(pieces);
    }
    let m_re_par = median(t_re_par);
    println!(
        "regex split (rayon)          : {m_re_par:>8.1} ms  {:>6.1} MB/s  speedup={:.2}x",
        total_bytes as f64 / 1e6 / (m_re_par / 1000.0),
        m_re / m_re_par
    );

    // 6. The load distribution.
    let mut sizes: Vec<usize> = files.iter().map(|(_, s)| s.len()).collect();
    sizes.sort_unstable_by(|a, b| b.cmp(a));
    let mut acc = 0usize;
    for (i, s) in sizes.iter().enumerate() {
        acc += s;
        if i < 4 || (i + 1) % 2000 == 0 {
            println!(
                "  top {:>5} files = {:>10} bytes ({:>5.1}%)",
                i + 1,
                acc,
                acc as f64 / total_bytes as f64 * 100.0
            );
        }
    }
}
