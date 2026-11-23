//! Prints the collation of a list of pairs, so it can be compared with the
//! reference implementation (`String.prototype.localeCompare`).
//!
//! Usage: collcmp

use rustmix::util::path::locale_cmp;

fn main() {
    let pairs: &[(&str, &str)] = &[
        ("abc.ts", "åäö.ts"),
        ("åäö.ts", "abc.ts"),
        ("å", "a"),
        ("å", "z"),
        ("ö", "o"),
        ("ü", "u"),
        ("ä", "å"),
        ("é", "e"),
        ("COMMIT_EDITMSG", "config"),
        ("a", "A"),
        ("A", "a"),
        ("a", "Ab"),
        ("Ab", "a"),
        ("A", "ab"),
        ("a1", "a2"),
        ("Z", "a"),
        ("README.md", "readme.md"),
        ("a", "ab"),
        ("ab", "a"),
        ("_", "a"),
        ("-", "a"),
        ("a b", "a-b"),
        ("日本", "z"),
        ("z", "日本"),
        ("日本", "��本"),
        ("café", "cafe"),
        ("cafe", "café"),
        ("ünicode", "unicode"),
        ("Ω", "Ω"),
        ("ss", "ß"),
    ];
    for (a, b) in pairs {
        let order = match locale_cmp(a, b) {
            std::cmp::Ordering::Less => -1,
            std::cmp::Ordering::Equal => 0,
            std::cmp::Ordering::Greater => 1,
        };
        println!("{order}\t{a}\t{b}");
    }
}
