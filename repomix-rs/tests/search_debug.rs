use rustmix::util::glob::{normalize_glob_pattern, PatternSet};

#[test]
fn debug_patterns() {
    for p in [
        "**/rustmix-output.*",
        "**/repomix-output.*",
        "**/repopack-output.*",
        "**/package-lock.json",
        "**/Cargo.lock",
    ] {
        let normalized = normalize_glob_pattern(p);
        let mut set = PatternSet::new();
        set.add(&normalized);
        println!(
            "{p} -> {normalized} matches rustmix-output.txt: {}",
            set.is_match("rustmix-output.txt")
        );
    }
}
