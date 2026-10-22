//! Prints the token count of every file in a directory, one per line.
//!
//! Used to compare the tokenizer against the reference implementation
//! (gpt-tokenizer) file by file.
//!
//! Usage: tokdiff <dir>

use std::path::PathBuf;

fn walk(dir: &PathBuf, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, out);
        } else {
            out.push(path);
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: tokdiff <dir>");
        std::process::exit(2);
    }
    let bpe = tiktoken_rs::o200k_base_singleton();
    let mut files = Vec::new();
    walk(&PathBuf::from(&args[0]), &mut files);
    files.sort();
    let mut total = 0usize;
    for file in &files {
        let Ok(text) = std::fs::read_to_string(file) else {
            continue;
        };
        let n = bpe.encode_ordinary(&text).len();
        total += n;
        println!("{n}\t{}", file.display());
    }
    eprintln!("TOTAL {total}");
}
