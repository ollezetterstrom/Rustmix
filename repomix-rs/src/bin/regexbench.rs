//! Measures the fixed cost of compiling the secret-detection rules.
//!
//! The rules are compiled eagerly on the first scan, so their compile time is
//! paid by every run - even a run over a single tiny file.
use std::time::Instant;

use regex::Regex;
use repomix_rs::core::security;

fn main() {
    let patterns = security::rule_patterns();
    println!("rules: {}", patterns.len());

    let mut rows: Vec<(f64, String, String)> = Vec::new();
    let total = Instant::now();
    for (name, pattern) in &patterns {
        let start = Instant::now();
        let re = Regex::new(pattern).ok();
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        println!(
            "{:>9.3} ms  {:<12} len={:<5} {}",
            ms,
            name,
            pattern.len(),
            re.as_ref().map(|_| "").unwrap_or("COMPILE FAILED")
        );
        rows.push((ms, name.to_string(), pattern.to_string()));
    }
    let total_ms = total.elapsed().as_secs_f64() * 1000.0;

    rows.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    println!("\n--- slowest 8 ---");
    for (ms, name, pattern) in rows.iter().take(8) {
        let short: String = pattern.chars().take(70).collect();
        println!("{ms:>9.3} ms  {name:<12} {short}");
    }
    println!("\nTOTAL compile time: {total_ms:.3} ms");
}
