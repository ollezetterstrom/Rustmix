//! Prints the token count of every prefix of a file (in characters).
//!
//! Used to find the first position where the tokenizer disagrees with the
//! reference implementation.
//!
//! Usage: tokprefix <file>

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: tokprefix <file>");
        std::process::exit(2);
    }
    let bpe = tiktoken_rs::o200k_base_singleton();
    let text = std::fs::read_to_string(&args[0]).unwrap();
    let chars: Vec<char> = text.chars().collect();
    for n in 0..=chars.len() {
        let prefix: String = chars[..n].iter().collect();
        println!("{n}\t{}", bpe.encode_ordinary(&prefix).len());
    }
}
