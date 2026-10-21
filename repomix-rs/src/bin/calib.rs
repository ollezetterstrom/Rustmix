// Calibration harness: compares the Rust secret scanner against secretlint.
use std::io::Read;

fn main() {
    let path = std::env::args().nth(1).expect("usage: calib <file>");
    let mut content = String::new();
    std::fs::File::open(&path)
        .unwrap()
        .read_to_string(&mut content)
        .unwrap();
    let name = std::path::Path::new(&path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let issues = repomix_rs::core::security::count_issues(&content);
    println!("{name}\t{issues}");
}
