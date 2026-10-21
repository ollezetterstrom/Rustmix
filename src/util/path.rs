//! Path normalization helpers.
//!
//! Every path that reaches the output pipeline is normalized to POSIX
//! forward slashes, regardless of the host platform. This is a hard
//! requirement of the Repomix output format.

use std::cmp::Ordering;

/// Convert Windows separators to POSIX ones.
#[inline]
pub fn to_posix(path: &str) -> String {
    if path.contains('\\') {
        path.replace('\\', "/")
    } else {
        path.to_string()
    }
}

/// Split a path into segments, accepting both `/` and `\` as separators.
pub fn split_segments(path: &str) -> Vec<&str> {
    path.split(|c| c == '/' || c == '\\').collect()
}

/// Approximation of the ICU root collation used by JavaScript's
/// `String.prototype.localeCompare`.
///
/// The ordering classes follow the Unicode Collation Algorithm root locale:
/// whitespace < punctuation < symbols < digits < letters. Within a letter,
/// the lowercase form sorts before the uppercase form (case is a tertiary
/// difference in the root locale).
fn collation_key(c: char) -> (u32, u32) {
    match c {
        'a'..='z' => (0x0500 + (c as u32 - 'a' as u32), 0),
        'A'..='Z' => (0x0500 + (c as u32 - 'A' as u32), 1),
        '0'..='9' => (0x0400 + (c as u32 - '0' as u32), 0),
        '\t'..='\r' | ' ' => (0x0100 + c as u32, 0),
        // General punctuation
        '!' | '"' | '#' | '%' | '&' | '\'' | '(' | ')' | '*' | ',' | '.' | '/' | ':' | ';'
        | '?' | '@' | '[' | '\\' | ']' | '_' | '{' | '}' => (0x0200 + c as u32, 0),
        // Symbols
        '$' | '+' | '<' | '=' | '>' | '^' | '`' | '|' | '~' => (0x0300 + c as u32, 0),
        c if c.is_ascii() => (0x0300 + c as u32, 0),
        c => {
            let lower = c.to_lowercase().next().unwrap_or(c);
            if lower.is_ascii_lowercase() {
                (0x0500 + 26 + (lower as u32 - 'a' as u32), 0)
            } else {
                (0x0600 + c as u32, 0)
            }
        }
    }
}

/// Compare two strings the way `localeCompare` would.
pub fn locale_cmp(a: &str, b: &str) -> Ordering {
    let mut ai = a.chars();
    let mut bi = b.chars();
    loop {
        match (ai.next(), bi.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => {
                if x == y {
                    continue;
                }
                let (px, sx) = collation_key(x);
                let (py, sy) = collation_key(y);
                if px != py {
                    return px.cmp(&py);
                }
                if sx != sy {
                    return sx.cmp(&sy);
                }
            }
        }
    }
}

/// Compare two path strings using Repomix's `sortPaths` semantics.
///
/// Paths are compared segment by segment. At the first differing segment a
/// path that *ends* there sorts **after** one that continues, so files are
/// listed below the directories that contain them.
pub fn cmp_paths(a: &str, b: &str) -> Ordering {
    let pa = split_segments(a);
    let pb = split_segments(b);
    let n = pa.len().min(pb.len());
    for i in 0..n {
        if pa[i] != pb[i] {
            let is_last_a = i + 1 == pa.len();
            let is_last_b = i + 1 == pb.len();
            if !is_last_a && is_last_b {
                return Ordering::Less;
            }
            if is_last_a && !is_last_b {
                return Ordering::Greater;
            }
            return locale_cmp(pa[i], pb[i]);
        }
    }
    pa.len().cmp(&pb.len())
}

/// Sort a list of paths using Repomix's `sortPaths` semantics (stable).
pub fn sort_paths(paths: &mut Vec<String>) {
    paths.sort_by(|a, b| cmp_paths(a, b));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_windows_separators() {
        assert_eq!(to_posix("src\\main.rs"), "src/main.rs");
        assert_eq!(to_posix("src/main.rs"), "src/main.rs");
    }

    #[test]
    fn files_sort_after_their_directories() {
        let mut v = vec![
            "src/b/c.txt".to_string(),
            "src/a.txt".to_string(),
            "src".to_string(),
            "a.txt".to_string(),
        ];
        sort_paths(&mut v);
        assert_eq!(v, vec!["a.txt", "src", "src/a.txt", "src/b/c.txt"]);
    }

    #[test]
    fn locale_ordering_matches_icu_root() {
        // ICU root: lowercase sorts before uppercase for the same letter.
        assert_eq!(locale_cmp("a", "A"), Ordering::Less);
        assert_eq!(locale_cmp("A", "b"), Ordering::Less);
        assert_eq!(locale_cmp("a1", "a2"), Ordering::Less);
        assert_eq!(locale_cmp("Z", "a"), Ordering::Greater);
    }
}
