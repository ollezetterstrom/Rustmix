use repomix_rs::util::glob::{normalize_glob_pattern, PatternSet};

#[test]
fn debug_patterns() {
    for p in [
        "**/repomix-output.*",
        "**/repopack-output.*",
        "**/package-lock.json",
        "**/Cargo.lock",
    ] {
        let normalized = normalize_glob_pattern(p);
        let mut set = PatternSet::new();
        set.add(&normalized);
        println!(
            "{p} -> {normalized} matches repomix-output.xml: {}",
            set.is_match("repomix-output.xml")
        );
    }
}
