//! Micro-benchmarks for the hot primitives.
use std::time::Instant;

use repomix_rs::core::metrics;
use repomix_rs::core::security;
use repomix_rs::util::binary;

fn bench<T>(label: &str, iters: u32, mut f: impl FnMut() -> T) {
    // warmup
    f();
    let start = Instant::now();
    for _ in 0..iters {
        std::hint::black_box(f());
    }
    let elapsed = start.elapsed().as_secs_f64() * 1000.0 / iters as f64;
    println!("{label:<44} {elapsed:>10.3} ms/iter");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| "src/core/packager.rs".to_string());
    let content = std::fs::read_to_string(&path).unwrap_or_default();
    println!("sample: {path} ({} bytes)", content.len());

    // --- tokenizer ------------------------------------------------------
    let start = Instant::now();
    let bpe = tiktoken_rs::o200k_base_singleton();
    println!(
        "{:<44} {:>10.3} ms  (BPE init, one time)",
        "tiktoken o200k_base init",
        start.elapsed().as_secs_f64() * 1000.0
    );

    let reps = 20;
    let start = Instant::now();
    let mut total = 0usize;
    for _ in 0..reps {
        total += bpe.encode_ordinary(&content).len();
    }
    let per = start.elapsed().as_secs_f64() * 1000.0 / reps as f64;
    println!(
        "{:<44} {:>10.3} ms  ({:.1} MB/s, {total} tokens)",
        "tiktoken encode_ordinary (1x)",
        per,
        content.len() as f64 / 1_048_576.0 / (per / 1000.0)
    );

    // --- security scan ----------------------------------------------------
    bench("security scan (all rules)", 20, || {
        security::count_issues(&content)
    });

    // --- binary detection --------------------------------------------------
    let bytes = content.as_bytes();
    bench("binary inspect (simd utf8 + null probe)", 20, || {
        binary::inspect(bytes)
    });

    // --- token counting via the crate API -----------------------------------
    bench("metrics::count_tokens", 20, || {
        metrics::count_tokens(&content, repomix_rs::cli::TokenEncoding::O200kBase)
    });
}
