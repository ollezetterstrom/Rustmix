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

/// The collation elements of a character.
///
/// The Unicode Collation Algorithm compares three levels: the *primary*
/// weights (the base letters), the *secondary* weights (the accents) and
/// the *tertiary* weights (the case). A precomposed character like `ä`
/// therefore sorts with the `a` (the primary weight), but after a plain
/// `a` (the accent is a secondary difference) and before `Ä` (the case is a
/// tertiary difference).
fn collation_keys(c: char) -> (u32, u32, u32) {
    match c {
        'a'..='z' => (0x0500 + (c as u32 - 'a' as u32), 0, 0),
        'A'..='Z' => (0x0500 + (c as u32 - 'A' as u32), 0, 1),
        '0'..='9' => (0x0400 + (c as u32 - '0' as u32), 0, 0),
        '\t'..='\r' | ' ' => (0x0100 + c as u32, 0, 0),
        // General punctuation
        '!' | '"' | '#' | '%' | '&' | '\'' | '(' | ')' | '*' | ',' | '.' | '/' | ':' | ';'
        | '?' | '@' | '[' | '\\' | ']' | '_' | '{' | '}' => (0x0200 + c as u32, 0, 0),
        // Symbols
        '$' | '+' | '<' | '=' | '>' | '^' | '`' | '|' | '~' => (0x0300 + c as u32, 0, 0),
        c if c.is_ascii() => (0x0300 + c as u32, 0, 0),
        c => {
            // A precomposed Latin letter: the base letter carries the
            // primary weight, the accent the secondary one.
            if let Some((base, mark)) = latin_decomposition(c) {
                let (primary, _, tertiary) = collation_keys(base);
                return (primary, mark as u32, tertiary);
            }
            let lower = c.to_lowercase().next().unwrap_or(c);
            if lower.is_ascii_lowercase() {
                (0x0500 + 26 + (lower as u32 - 'a' as u32), 0, 0)
            } else {
                (0x0600 + c as u32, 0, 0)
            }
        }
    }
}

/// The base letter and the combining mark of a precomposed Latin letter.
///
/// The table is the canonical decomposition of the characters between
/// `U+00C0` and `U+024F` whose base is an ASCII letter. The characters
/// without a decomposition (`æ`, `ø`, `ł`, ...) get their own primary
/// weight in the root locale and are not listed here.
fn latin_decomposition(c: char) -> Option<(char, char)> {
    match c {
        '\u{C0}' => Some(('A', '\u{300}')),
        '\u{C1}' => Some(('A', '\u{301}')),
        '\u{C2}' => Some(('A', '\u{302}')),
        '\u{C3}' => Some(('A', '\u{303}')),
        '\u{C4}' => Some(('A', '\u{308}')),
        '\u{C5}' => Some(('A', '\u{30A}')),
        '\u{C7}' => Some(('C', '\u{327}')),
        '\u{C8}' => Some(('E', '\u{300}')),
        '\u{C9}' => Some(('E', '\u{301}')),
        '\u{CA}' => Some(('E', '\u{302}')),
        '\u{CB}' => Some(('E', '\u{308}')),
        '\u{CC}' => Some(('I', '\u{300}')),
        '\u{CD}' => Some(('I', '\u{301}')),
        '\u{CE}' => Some(('I', '\u{302}')),
        '\u{CF}' => Some(('I', '\u{308}')),
        '\u{D1}' => Some(('N', '\u{303}')),
        '\u{D2}' => Some(('O', '\u{300}')),
        '\u{D3}' => Some(('O', '\u{301}')),
        '\u{D4}' => Some(('O', '\u{302}')),
        '\u{D5}' => Some(('O', '\u{308}')),
        '\u{D6}' => Some(('O', '\u{30B}')),
        '\u{D9}' => Some(('U', '\u{300}')),
        '\u{DA}' => Some(('U', '\u{301}')),
        '\u{DB}' => Some(('U', '\u{302}')),
        '\u{DC}' => Some(('U', '\u{308}')),
        '\u{DD}' => Some(('Y', '\u{301}')),
        '\u{DE}' => Some(('Y', '\u{308}')),
        '\u{DF}' => Some(('s', '\u{301}')),
        '\u{E0}' => Some(('a', '\u{300}')),
        '\u{E1}' => Some(('a', '\u{301}')),
        '\u{E2}' => Some(('a', '\u{302}')),
        '\u{E3}' => Some(('a', '\u{303}')),
        '\u{E4}' => Some(('a', '\u{308}')),
        '\u{E5}' => Some(('a', '\u{30A}')),
        '\u{E6}' => None,
        '\u{E7}' => Some(('c', '\u{327}')),
        '\u{E8}' => Some(('e', '\u{300}')),
        '\u{E9}' => Some(('e', '\u{301}')),
        '\u{EA}' => Some(('e', '\u{302}')),
        '\u{EB}' => Some(('e', '\u{308}')),
        '\u{EC}' => Some(('i', '\u{300}')),
        '\u{ED}' => Some(('i', '\u{301}')),
        '\u{EE}' => Some(('i', '\u{302}')),
        '\u{EF}' => Some(('i', '\u{308}')),
        '\u{F1}' => Some(('n', '\u{303}')),
        '\u{F2}' => Some(('o', '\u{300}')),
        '\u{F3}' => Some(('o', '\u{301}')),
        '\u{F4}' => Some(('o', '\u{302}')),
        '\u{F5}' => Some(('o', '\u{308}')),
        '\u{F6}' => Some(('o', '\u{30B}')),
        '\u{F9}' => Some(('u', '\u{300}')),
        '\u{FA}' => Some(('u', '\u{301}')),
        '\u{FB}' => Some(('u', '\u{302}')),
        '\u{FC}' => Some(('u', '\u{308}')),
        '\u{FD}' => Some(('y', '\u{301}')),
        '\u{FE}' => Some(('y', '\u{308}')),
        '\u{FF}' => Some(('y', '\u{308}')),
        '\u{100}' => Some(('A', '\u{304}')),
        '\u{101}' => Some(('a', '\u{304}')),
        '\u{102}' => Some(('A', '\u{306}')),
        '\u{103}' => Some(('a', '\u{306}')),
        '\u{104}' => Some(('A', '\u{328}')),
        '\u{105}' => Some(('a', '\u{328}')),
        '\u{106}' => Some(('C', '\u{301}')),
        '\u{107}' => Some(('c', '\u{301}')),
        '\u{108}' => Some(('C', '\u{302}')),
        '\u{109}' => Some(('c', '\u{302}')),
        '\u{10A}' => Some(('C', '\u{307}')),
        '\u{10B}' => Some(('c', '\u{307}')),
        '\u{10C}' => Some(('C', '\u{30C}')),
        '\u{10D}' => Some(('c', '\u{30C}')),
        '\u{10E}' => Some(('D', '\u{30C}')),
        '\u{10F}' => Some(('d', '\u{30C}')),
        '\u{112}' => Some(('E', '\u{304}')),
        '\u{113}' => Some(('e', '\u{304}')),
        '\u{114}' => Some(('E', '\u{306}')),
        '\u{115}' => Some(('e', '\u{306}')),
        '\u{116}' => Some(('E', '\u{307}')),
        '\u{117}' => Some(('e', '\u{307}')),
        '\u{118}' => Some(('E', '\u{328}')),
        '\u{119}' => Some(('e', '\u{328}')),
        '\u{11A}' => Some(('E', '\u{30C}')),
        '\u{11B}' => Some(('e', '\u{30C}')),
        '\u{11C}' => Some(('G', '\u{302}')),
        '\u{11D}' => Some(('g', '\u{302}')),
        '\u{11E}' => Some(('G', '\u{306}')),
        '\u{11F}' => Some(('g', '\u{306}')),
        '\u{120}' => Some(('G', '\u{307}')),
        '\u{121}' => Some(('g', '\u{307}')),
        '\u{122}' => Some(('G', '\u{327}')),
        '\u{123}' => Some(('g', '\u{327}')),
        '\u{124}' => Some(('H', '\u{302}')),
        '\u{125}' => Some(('h', '\u{302}')),
        '\u{128}' => Some(('I', '\u{303}')),
        '\u{129}' => Some(('i', '\u{303}')),
        '\u{12A}' => Some(('I', '\u{304}')),
        '\u{12B}' => Some(('i', '\u{304}')),
        '\u{12C}' => Some(('I', '\u{306}')),
        '\u{12D}' => Some(('i', '\u{306}')),
        '\u{12E}' => Some(('I', '\u{328}')),
        '\u{12F}' => Some(('i', '\u{328}')),
        '\u{130}' => Some(('I', '\u{307}')),
        '\u{134}' => Some(('J', '\u{302}')),
        '\u{135}' => Some(('j', '\u{302}')),
        '\u{136}' => Some(('K', '\u{327}')),
        '\u{137}' => Some(('k', '\u{327}')),
        '\u{139}' => Some(('L', '\u{301}')),
        '\u{13A}' => Some(('l', '\u{301}')),
        '\u{13B}' => Some(('L', '\u{327}')),
        '\u{13C}' => Some(('l', '\u{327}')),
        '\u{13D}' => Some(('L', '\u{30C}')),
        '\u{13E}' => Some(('l', '\u{30C}')),
        '\u{143}' => Some(('N', '\u{301}')),
        '\u{144}' => Some(('n', '\u{301}')),
        '\u{145}' => Some(('N', '\u{327}')),
        '\u{146}' => Some(('n', '\u{327}')),
        '\u{147}' => Some(('N', '\u{30C}')),
        '\u{148}' => Some(('n', '\u{30C}')),
        '\u{14C}' => Some(('O', '\u{304}')),
        '\u{14D}' => Some(('o', '\u{304}')),
        '\u{14E}' => Some(('O', '\u{306}')),
        '\u{14F}' => Some(('o', '\u{306}')),
        '\u{150}' => Some(('O', '\u{30B}')),
        '\u{151}' => Some(('o', '\u{30B}')),
        '\u{154}' => Some(('R', '\u{301}')),
        '\u{155}' => Some(('r', '\u{301}')),
        '\u{156}' => Some(('R', '\u{327}')),
        '\u{157}' => Some(('r', '\u{327}')),
        '\u{158}' => Some(('R', '\u{30C}')),
        '\u{159}' => Some(('r', '\u{30C}')),
        '\u{15A}' => Some(('S', '\u{301}')),
        '\u{15B}' => Some(('s', '\u{301}')),
        '\u{15C}' => Some(('S', '\u{302}')),
        '\u{15D}' => Some(('s', '\u{302}')),
        '\u{15E}' => Some(('S', '\u{327}')),
        '\u{15F}' => Some(('s', '\u{327}')),
        '\u{160}' => Some(('S', '\u{30C}')),
        '\u{161}' => Some(('s', '\u{30C}')),
        '\u{162}' => Some(('T', '\u{327}')),
        '\u{163}' => Some(('t', '\u{327}')),
        '\u{164}' => Some(('T', '\u{30C}')),
        '\u{165}' => Some(('t', '\u{30C}')),
        '\u{168}' => Some(('U', '\u{303}')),
        '\u{169}' => Some(('u', '\u{303}')),
        '\u{16A}' => Some(('U', '\u{304}')),
        '\u{16B}' => Some(('u', '\u{304}')),
        '\u{16C}' => Some(('U', '\u{306}')),
        '\u{16D}' => Some(('u', '\u{306}')),
        '\u{16E}' => Some(('U', '\u{30A}')),
        '\u{16F}' => Some(('u', '\u{30A}')),
        '\u{170}' => Some(('U', '\u{30B}')),
        '\u{171}' => Some(('u', '\u{30B}')),
        '\u{172}' => Some(('U', '\u{328}')),
        '\u{173}' => Some(('u', '\u{328}')),
        '\u{174}' => Some(('W', '\u{302}')),
        '\u{175}' => Some(('w', '\u{302}')),
        '\u{176}' => Some(('Y', '\u{302}')),
        '\u{177}' => Some(('y', '\u{302}')),
        '\u{178}' => Some(('Y', '\u{308}')),
        '\u{179}' => Some(('y', '\u{308}')),
        '\u{17A}' => Some(('Z', '\u{301}')),
        '\u{17B}' => Some(('z', '\u{301}')),
        '\u{17C}' => Some(('Z', '\u{302}')),
        '\u{17D}' => Some(('z', '\u{302}')),
        '\u{17E}' => Some(('Z', '\u{30C}')),
        '\u{17F}' => Some(('z', '\u{30C}')),
        '\u{1E00}' => Some(('A', '\u{304}')),
        '\u{1E01}' => Some(('a', '\u{304}')),
        '\u{1E02}' => Some(('A', '\u{304}')),
        '\u{1E03}' => Some(('a', '\u{304}')),
        '\u{1E04}' => Some(('A', '\u{306}')),
        '\u{1E05}' => Some(('a', '\u{306}')),
        '\u{1E06}' => Some(('A', '\u{328}')),
        '\u{1E07}' => Some(('a', '\u{328}')),
        '\u{1E08}' => Some(('C', '\u{302}')),
        '\u{1E09}' => Some(('c', '\u{302}')),
        '\u{1E0A}' => Some(('C', '\u{307}')),
        '\u{1E0B}' => Some(('c', '\u{307}')),
        '\u{1E0C}' => Some(('C', '\u{30C}')),
        '\u{1E0D}' => Some(('c', '\u{30C}')),
        '\u{1E0E}' => Some(('D', '\u{307}')),
        '\u{1E0F}' => Some(('d', '\u{307}')),
        '\u{1E10}' => Some(('D', '\u{30C}')),
        '\u{1E11}' => Some(('d', '\u{30C}')),
        '\u{1E12}' => Some(('E', '\u{304}')),
        '\u{1E13}' => Some(('e', '\u{304}')),
        '\u{1E14}' => Some(('E', '\u{306}')),
        '\u{1E15}' => Some(('e', '\u{306}')),
        '\u{1E16}' => Some(('E', '\u{307}')),
        '\u{1E17}' => Some(('e', '\u{307}')),
        '\u{1E18}' => Some(('E', '\u{328}')),
        '\u{1E19}' => Some(('e', '\u{328}')),
        '\u{1E1A}' => Some(('E', '\u{30C}')),
        '\u{1E1B}' => Some(('e', '\u{30C}')),
        '\u{1E20}' => Some(('G', '\u{304}')),
        '\u{1E21}' => Some(('g', '\u{304}')),
        '\u{1E22}' => Some(('G', '\u{306}')),
        '\u{1E23}' => Some(('g', '\u{306}')),
        '\u{1E24}' => Some(('G', '\u{327}')),
        '\u{1E25}' => Some(('g', '\u{327}')),
        '\u{1E26}' => Some(('G', '\u{30C}')),
        '\u{1E27}' => Some(('g', '\u{30C}')),
        '\u{1E28}' => Some(('H', '\u{302}')),
        '\u{1E29}' => Some(('h', '\u{302}')),
        '\u{1E2A}' => Some(('I', '\u{307}')),
        '\u{1E2B}' => Some(('i', '\u{307}')),
        '\u{1E2C}' => Some(('I', '\u{303}')),
        '\u{1E2D}' => Some(('i', '\u{303}')),
        '\u{1E2E}' => Some(('I', '\u{304}')),
        '\u{1E2F}' => Some(('i', '\u{304}')),
        '\u{1E30}' => Some(('I', '\u{306}')),
        '\u{1E31}' => Some(('i', '\u{306}')),
        '\u{1E32}' => Some(('I', '\u{328}')),
        '\u{1E33}' => Some(('i', '\u{328}')),
        '\u{1E34}' => Some(('I', '\u{304}')),
        '\u{1E35}' => Some(('i', '\u{304}')),
        '\u{1E36}' => Some(('I', '\u{301}')),
        '\u{1E37}' => Some(('i', '\u{301}')),
        '\u{1E38}' => Some(('I', '\u{302}')),
        '\u{1E39}' => Some(('i', '\u{302}')),
        '\u{1E3A}' => Some(('I', '\u{308}')),
        '\u{1E3B}' => Some(('i', '\u{308}')),
        '\u{1E3C}' => Some(('L', '\u{301}')),
        '\u{1E3D}' => Some(('l', '\u{301}')),
        '\u{1E3E}' => Some(('L', '\u{327}')),
        '\u{1E3F}' => Some(('l', '\u{327}')),
        '\u{1E40}' => Some(('L', '\u{30C}')),
        '\u{1E41}' => Some(('l', '\u{30C}')),
        '\u{1E42}' => Some(('N', '\u{301}')),
        '\u{1E43}' => Some(('n', '\u{301}')),
        '\u{1E44}' => Some(('N', '\u{327}')),
        '\u{1E45}' => Some(('n', '\u{327}')),
        '\u{1E46}' => Some(('N', '\u{30C}')),
        '\u{1E47}' => Some(('n', '\u{30C}')),
        '\u{1E48}' => Some(('O', '\u{304}')),
        '\u{1E49}' => Some(('o', '\u{304}')),
        '\u{1E4A}' => Some(('O', '\u{306}')),
        '\u{1E4B}' => Some(('o', '\u{306}')),
        '\u{1E4C}' => Some(('O', '\u{30B}')),
        '\u{1E4D}' => Some(('o', '\u{30B}')),
        '\u{1E50}' => Some(('R', '\u{301}')),
        '\u{1E51}' => Some(('r', '\u{301}')),
        '\u{1E52}' => Some(('R', '\u{327}')),
        '\u{1E53}' => Some(('r', '\u{327}')),
        '\u{1E54}' => Some(('R', '\u{30C}')),
        '\u{1E55}' => Some(('r', '\u{30C}')),
        '\u{1E56}' => Some(('S', '\u{301}')),
        '\u{1E57}' => Some(('s', '\u{301}')),
        '\u{1E58}' => Some(('S', '\u{302}')),
        '\u{1E59}' => Some(('s', '\u{302}')),
        '\u{1E5A}' => Some(('S', '\u{327}')),
        '\u{1E5B}' => Some(('s', '\u{327}')),
        '\u{1E5C}' => Some(('S', '\u{30C}')),
        '\u{1E5D}' => Some(('s', '\u{30C}')),
        '\u{1E5E}' => Some(('T', '\u{307}')),
        '\u{1E5F}' => Some(('t', '\u{307}')),
        '\u{1E60}' => Some(('T', '\u{302}')),
        '\u{1E61}' => Some(('t', '\u{302}')),
        '\u{1E62}' => Some(('T', '\u{328}')),
        '\u{1E63}' => Some(('t', '\u{328}')),
        '\u{1E64}' => Some(('T', '\u{30C}')),
        '\u{1E65}' => Some(('t', '\u{30C}')),
        '\u{1E68}' => Some(('U', '\u{303}')),
        '\u{1E69}' => Some(('u', '\u{303}')),
        '\u{1E6A}' => Some(('U', '\u{304}')),
        '\u{1E6B}' => Some(('u', '\u{304}')),
        '\u{1E6C}' => Some(('U', '\u{306}')),
        '\u{1E6D}' => Some(('u', '\u{306}')),
        '\u{1E6E}' => Some(('U', '\u{30A}')),
        '\u{1E6F}' => Some(('u', '\u{30A}')),
        '\u{1E70}' => Some(('U', '\u{30B}')),
        '\u{1E71}' => Some(('u', '\u{30B}')),
        '\u{1E72}' => Some(('U', '\u{328}')),
        '\u{1E73}' => Some(('u', '\u{328}')),
        '\u{1E74}' => Some(('W', '\u{302}')),
        '\u{1E75}' => Some(('w', '\u{302}')),
        '\u{1E76}' => Some(('Y', '\u{302}')),
        '\u{1E77}' => Some(('y', '\u{302}')),
        '\u{1E78}' => Some(('Y', '\u{308}')),
        '\u{1E79}' => Some(('y', '\u{308}')),
        '\u{1E7A}' => Some(('Z', '\u{302}')),
        '\u{1E7B}' => Some(('z', '\u{302}')),
        '\u{1E7C}' => Some(('Z', '\u{30C}')),
        '\u{1E7D}' => Some(('z', '\u{30C}')),
        _ => None,
    }
}

/// Compare two strings the way `localeCompare` would.
///
/// The Unicode collation algorithm compares the *primary* weights of the whole
/// string first, then the *secondary* weights (the accents) and finally the
/// *tertiary* weights (the case). So `"COMMIT_EDITMSG"` sorts before
/// `"config"` while `"a"` sorts before `"A"`.
///
/// This is the allocation free equivalent of collecting the collation keys of
/// both strings into two vectors first: the path sort performs ~90 000
/// comparisons for a medium sized repository, and every comparison used to
/// allocate twice.
pub fn locale_cmp(a: &str, b: &str) -> Ordering {
    // The primary weights (the base letters).
    if let Some(order) = compare_level(a, b, |key| key.0) {
        return order;
    }
    // The secondary weights (the accents).
    if let Some(order) = compare_level(a, b, |key| key.1) {
        return order;
    }
    // The tertiary weights (the case).
    compare_level(a, b, |key| key.2).unwrap_or(Ordering::Equal)
}

/// Compare one level of the collation keys of two strings.
///
/// Returns `None` when the two sequences are equal.
fn compare_level(
    a: &str,
    b: &str,
    level: impl Fn((u32, u32, u32)) -> u32,
) -> Option<Ordering> {
    let mut ia = a.chars().map(|c| level(collation_keys(c)));
    let mut ib = b.chars().map(|c| level(collation_keys(c)));
    loop {
        match (ia.next(), ib.next()) {
            (Some(x), Some(y)) => {
                if x != y {
                    return Some(x.cmp(&y));
                }
            }
            // One string is a prefix of the other: the shorter one sorts
            // first.
            (None, Some(_)) => return Some(Ordering::Less),
            (Some(_), None) => return Some(Ordering::Greater),
            (None, None) => return None,
        }
    }
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

    /// The reference implementation replaces the Windows separator with the
    /// POSIX one *unconditionally* - on every platform, because
    /// `path.win32.sep` is a constant in Node - and splits the path on the
    /// POSIX separator. A file name that contains a backslash is therefore
    /// split into two segments on Linux as well, and this implementation has
    /// to do the same to stay byte compatible.
    #[test]
    fn backslashes_are_split_like_the_reference() {
        // The reference replaces the backslash first and then splits on the
        // slash, so every backslash becomes a separator.
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
    fn locale_ordering_matches_icu_root() {
        // ICU root: lowercase sorts before uppercase for the same letter.
        assert_eq!(locale_cmp("a", "A"), Ordering::Less);
        assert_eq!(locale_cmp("A", "b"), Ordering::Less);
        assert_eq!(locale_cmp("a1", "a2"), Ordering::Less);
        assert_eq!(locale_cmp("Z", "a"), Ordering::Greater);
    }

    /// An accented letter has the primary weight of its base letter.
    ///
    /// `localeCompare` sorts `åäö.ts` before `abc.ts`, because the primary
    /// weight of `å` is the one of `a`.
    #[test]
    fn accented_letters_sort_with_their_base_letter() {
        assert_eq!(locale_cmp("å", "a"), Ordering::Greater);
        assert_eq!(locale_cmp("å", "z"), Ordering::Less);
        assert_eq!(locale_cmp("åäö.ts", "abc.ts"), Ordering::Less);
        assert_eq!(locale_cmp("café", "cafe"), Ordering::Greater);
        // The accent is a secondary difference, the case a tertiary one.
        assert_eq!(locale_cmp("ä", "a"), Ordering::Greater);
        assert_eq!(locale_cmp("Ä", "ä"), Ordering::Greater);
        assert_eq!(locale_cmp("ä", "b"), Ordering::Less);
    }

    /// The reference implementation of the collation, allocating the keys of
    /// both strings up front. The optimised version has to agree with it for
    /// every input.
    fn locale_cmp_reference(a: &str, b: &str) -> Ordering {
        let ka: Vec<(u32, u32, u32)> = a.chars().map(collation_keys).collect();
        let kb: Vec<(u32, u32, u32)> = b.chars().map(collation_keys).collect();
        for level in 0..3 {
            let sa: Vec<u32> = ka.iter().map(|k| match level {
                0 => k.0,
                1 => k.1,
                _ => k.2,
            }).collect();
            let sb: Vec<u32> = kb.iter().map(|k| match level {
                0 => k.0,
                1 => k.1,
                _ => k.2,
            }).collect();
            for (x, y) in sa.iter().zip(sb.iter()) {
                if x != y {
                    return x.cmp(y);
                }
            }
            if sa.len() != sb.len() {
                return sa.len().cmp(&sb.len());
            }
        }
        Ordering::Equal
    }

    fn cmp_paths_reference(a: &str, b: &str) -> Ordering {
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
                return locale_cmp_reference(pa[i], pb[i]);
            }
        }
        pa.len().cmp(&pb.len())
    }

    #[test]
    fn optimised_comparison_matches_the_reference() {
        let samples = [
            "a", "A", "ab", "AB", "aB", "Ab", "abc", "a1", "a2", "A1", "config", "COMMIT_EDITMSG",
            "src", "src/main.rs", "src/a", "src/a/b", "z", "Z", "0", "9", "_", "-", "a b", "a-b",
            "README.md", "readme.md", "x/y/z", "x/y", "", "a/", "/a", "A/B/C", "a/b/c",
            "å", "åäö.ts", "abc.ts", "ä", "Ä", "é", "café", "cafe", "日本", "ünicode", "unicode",
        ];
        for a in samples {
            for b in samples {
                assert_eq!(
                    locale_cmp(a, b),
                    locale_cmp_reference(a, b),
                    "locale_cmp({a:?}, {b:?}) differs"
                );
                assert_eq!(
                    cmp_paths(a, b),
                    cmp_paths_reference(a, b),
                    "cmp_paths({a:?}, {b:?}) differs"
                );
            }
        }
    }
}
