//! Path normalization helpers.
//!
//! Every path that reaches the output pipeline is normalized to POSIX
//! forward slashes, regardless of the host platform.

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

/// Compare two strings using standard lexicographical comparison.
#[inline]
pub fn locale_cmp(a: &str, b: &str) -> Ordering {
    a.cmp(b)
}

/// Compare two path strings using Repomix's `sortPaths` semantics.
///
/// Paths are compared segment by segment. At the first differing segment a
/// path that *ends* there sorts **after** one that continues, so files are
/// listed below the directories that contain them.
pub fn cmp_paths(a: &str, b: &str) -> Ordering {
    let mut ia = a.split(['/', '\\']).peekable();
    let mut ib = b.split(['/', '\\']).peekable();

    loop {
        match (ia.next(), ib.next()) {
            (Some(sa), Some(sb)) => {
                if sa != sb {
                    let is_last_a = ia.peek().is_none();
                    let is_last_b = ib.peek().is_none();
                    if !is_last_a && is_last_b {
                        return Ordering::Less;
                    }
                    if is_last_a && !is_last_b {
                        return Ordering::Greater;
                    }
                    return locale_cmp(sa, sb);
                }
            }
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
        }
    }
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
    fn backslashes_are_split_like_the_reference() {
        assert_eq!(split_segments("a\\b/c"), vec!["a", "b", "c"]);
        assert_eq!(split_segments("a/b\\c"), vec!["a", "b", "c"]);
        assert_eq!(to_posix("a\\b\\c"), "a/b/c");
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
        assert_eq!(v, vec!["src", "src/b/c.txt", "src/a.txt", "a.txt"]);
    }

    #[test]
    fn plain_lexicographical_sort() {
        assert_eq!(locale_cmp("a", "b"), Ordering::Less);
        assert_eq!(locale_cmp("b", "a"), Ordering::Greater);
        assert_eq!(locale_cmp("same", "same"), Ordering::Equal);
        assert_eq!(locale_cmp("a", "A"), Ordering::Greater);
    }
}
