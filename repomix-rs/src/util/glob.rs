//! A small, dependency-free glob matcher that reproduces the semantics of
//! the `minimatch` package used by Repomix (via `globby` / `fast-glob`).
//!
//! Supported syntax:
//!
//! * `*` - matches any run of characters inside a single path segment
//! * `?` - matches exactly one character inside a single path segment
//! * `**` - as a whole segment, matches zero or more path segments
//! * `[abc]`, `[a-z]`, `[!abc]` - character classes
//! * `{a,b}` - brace expansion
//! * `\x` - literal escape
//! * `!pattern` - negation (handled by the caller, see [`PatternSet`])

/// A single path segment pattern.
#[derive(Debug, Clone)]
enum Segment {
    /// Literal text, possibly containing `*`/`?`/classes.
    Pattern(Vec<Token>),
    /// `**` - matches zero or more segments.
    GlobStar,
}

#[derive(Debug, Clone)]
enum Token {
    Char(char),
    Star,
    Any,
    Class { negated: bool, items: Vec<ClassItem> },
}

#[derive(Debug, Clone)]
enum ClassItem {
    Char(char),
    Range(char, char),
}

/// The literal parts of a path segment.
///
/// A segment pattern like `*.log` has the suffix `.log`, and a segment
/// pattern like `node_modules` is a literal. Both are *necessary* for a
/// match, so they can be used to skip a pattern without running the
/// matcher.
#[derive(Debug, Clone, Default)]
struct SegmentShape {
    /// The whole segment, when it has no wildcards.
    literal: Option<String>,
    /// The literal part at the beginning.
    prefix: Option<String>,
    /// The literal part at the end.
    suffix: Option<String>,
}

impl SegmentShape {
    /// The shape of a compiled segment.
    fn of(tokens: &[Token]) -> Self {
        let literal = literal_of(tokens);
        if literal.is_some() {
            return SegmentShape {
                literal,
                prefix: None,
                suffix: None,
            };
        }
        SegmentShape {
            literal: None,
            prefix: leading_literal(tokens),
            suffix: trailing_literal(tokens),
        }
    }

    /// Can the shape be used as a condition?
    fn is_usable(&self) -> bool {
        self.literal.is_some() || self.prefix.is_some() || self.suffix.is_some()
    }

    /// Can this segment match the shape?
    fn matches(&self, segment: &str) -> bool {
        if let Some(literal) = &self.literal {
            return segment == literal;
        }
        if let Some(prefix) = &self.prefix {
            if !segment.starts_with(prefix.as_str()) {
                return false;
            }
        }
        if let Some(suffix) = &self.suffix {
            if !segment.ends_with(suffix.as_str()) {
                return false;
            }
        }
        true
    }
}

/// A compiled glob pattern.
#[derive(Debug, Clone)]
pub struct Pattern {
    segments: Vec<Segment>,
    /// Number of segments (excluding globstars) that a matching path must have.
    pub min_depth: usize,
    /// Maximum path depth, or `None` when the pattern contains a globstar.
    pub max_depth: Option<usize>,
    /// Literal prefix required before the first globstar, if any.
    pub literal_prefix: Vec<String>,
    /// True when the pattern contains at least one globstar.
    pub has_globstar: bool,
    /// True when a trailing `/**` requires an actual slash because the
    /// preceding segment ends in a wildcard.
    trailing_needs_slash: bool,
    /// A literal segment that has to be somewhere in the path.
    ///
    /// This is the case for patterns like `**/node_modules/**`: the path has
    /// to contain the segment `node_modules`.
    required_shape: Option<SegmentShape>,
    /// The shape of the last segment of the pattern.
    ///
    /// The last segment of a path has to match the last segment of the
    /// pattern, so the literal parts of that segment are a condition for
    /// every path.
    last_shape: Option<SegmentShape>,
}

/// A set of patterns; a path matches when *any* pattern matches.
#[derive(Debug, Clone, Default)]
pub struct PatternSet {
    patterns: Vec<Pattern>,
}

impl PatternSet {
    pub fn new() -> Self {
        Self { patterns: Vec::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }

    /// Add a glob pattern. Brace expansion is applied, so one input pattern
    /// can produce several compiled patterns.
    pub fn add(&mut self, glob: &str) {
        for expanded in expand_braces(glob) {
            if let Some(p) = Pattern::compile(&expanded) {
                self.patterns.push(p);
            }
        }
    }

    pub fn add_all<I: IntoIterator<Item = S>, S: AsRef<str>>(&mut self, globs: I) {
        for g in globs {
            self.add(g.as_ref());
        }
    }

    /// Does any pattern in the set match the given path?
    pub fn is_match(&self, path: &str) -> bool {
        if self.patterns.is_empty() {
            return false;
        }
        let segments = crate::util::path::split_segments(path);
        self.is_match_segments(&segments)
    }

    /// Does any pattern in the set match the given path segments?
    ///
    /// The caller can split the path once and use it for several sets.
    pub fn is_match_segments(&self, segments: &[&str]) -> bool {
        self.patterns
            .iter()
            .any(|pattern| pattern.may_match(segments) && pattern.matches(segments))
    }

    /// Could a directory at `dir` (given as path segments) possibly contain a
    /// path matched by this set? Used to prune directory traversal.
    pub fn may_contain(&self, dir: &[&str]) -> bool {
        if self.patterns.is_empty() {
            return true;
        }
        self.patterns.iter().any(|p| p.may_contain(dir))
    }

    /// Is every path below `dir` matched by this set? Used to prune
    /// directories that are completely ignored.
    pub fn prunes_dir(&self, dir: &str) -> bool {
        if self.patterns.is_empty() {
            return false;
        }
        let segments = crate::util::path::split_segments(dir);
        self.patterns.iter().any(|p| p.prunes_dir(&segments))
    }
}

impl Pattern {
    /// Compile a glob into a pattern. Returns `None` for empty input.
    pub fn compile(glob: &str) -> Option<Pattern> {
        if glob.is_empty() {
            return None;
        }
        let raw_segments: Vec<&str> = glob.split('/').collect();
        let mut segments = Vec::with_capacity(raw_segments.len());
        let mut min_depth = 0usize;
        let mut has_globstar = false;

        for raw in &raw_segments {
            if *raw == "**" {
                segments.push(Segment::GlobStar);
                has_globstar = true;
            } else {
                segments.push(Segment::Pattern(compile_tokens(raw)));
                min_depth += 1;
            }
        }

        let max_depth = if has_globstar { None } else { Some(min_depth) };

        // Literal prefix: leading segments that contain no wildcards.
        let mut literal_prefix = Vec::new();
        for seg in &segments {
            match seg {
                Segment::GlobStar => break,
                Segment::Pattern(tokens) => {
                    if tokens
                        .iter()
                        .all(|t| matches!(t, Token::Char(c) if *c != '/'))
                    {
                        let mut s = String::new();
                        let mut literal = true;
                        for t in tokens {
                            match t {
                                Token::Char(c) => s.push(*c),
                                _ => {
                                    literal = false;
                                    break;
                                }
                            }
                        }
                        if literal {
                            literal_prefix.push(s);
                            continue;
                        }
                    }
                    break;
                }
            }
        }

        // A trailing `/**` only matches the path itself when the preceding
        // segment does not end in a wildcard (`a/**` matches `a`, `a*/**` does
        // not).
        let trailing_needs_slash = match segments.len() {
            0 | 1 => false,
            n => match &segments[n - 2] {
                Segment::Pattern(tokens) => {
                    matches!(tokens.last(), Some(Token::Star))
                }
                Segment::GlobStar => false,
            },
        };

        // A segment after the first globstar has to be somewhere in the path.
        let mut required_shape = None;
        let mut seen_globstar = false;
        for segment in &segments {
            match segment {
                Segment::GlobStar => seen_globstar = true,
                Segment::Pattern(tokens) => {
                    if seen_globstar {
                        let shape = SegmentShape::of(tokens);
                        if shape.is_usable() {
                            required_shape = Some(shape);
                        }
                    }
                }
            }
        }

        // The literal parts of the last segment.
        let last_shape = match segments.last() {
            Some(Segment::Pattern(tokens)) => {
                let shape = SegmentShape::of(tokens);
                if shape.is_usable() {
                    Some(shape)
                } else {
                    None
                }
            }
            _ => None,
        };

        Some(Pattern {
            segments,
            min_depth,
            max_depth,
            literal_prefix,
            has_globstar,
            trailing_needs_slash,
            required_shape,
            last_shape,
        })
    }

    /// A cheap check that has to succeed for the pattern to match.
    ///
    /// Every condition here is *necessary* for a match, so a pattern that
    /// fails them can be skipped. The exact matcher still decides.
    pub fn may_match(&self, segments: &[&str]) -> bool {
        // The number of segments.
        if segments.len() < self.min_depth {
            return false;
        }
        if let Some(max) = self.max_depth {
            if segments.len() != max {
                return false;
            }
        }
        // The literal prefix.
        if !self.literal_prefix.is_empty() {
            if segments.len() < self.literal_prefix.len() {
                return false;
            }
            for (index, want) in self.literal_prefix.iter().enumerate() {
                if segments[index] != want.as_str() {
                    return false;
                }
            }
        }
        // A segment that has to be somewhere in the path.
        if let Some(shape) = &self.required_shape {
            if !segments.iter().any(|segment| shape.matches(segment)) {
                return false;
            }
        }
        // The last segment of the path.
        if let Some(shape) = &self.last_shape {
            match segments.last() {
                Some(last) => {
                    if !shape.matches(last) {
                        return false;
                    }
                }
                // The pattern needs a last segment, the path has none.
                None => return false,
            }
        }
        true
    }

    pub fn matches(&self, path_segments: &[&str]) -> bool {
        match_segments(&self.segments, path_segments, self.trailing_needs_slash)
    }

    /// Does this pattern match every path below the given directory?
    ///
    /// This is true for patterns of the form `some/dir/**`.
    pub fn prunes_dir(&self, dir: &[&str]) -> bool {
        let Some((last, prefix)) = self.segments.split_last() else {
            return false;
        };
        if !matches!(last, Segment::GlobStar) {
            return false;
        }
        match_segments(prefix, dir, false)
    }

    /// Conservative check: could this pattern match a path *inside* the given
    /// directory?
    pub fn may_contain(&self, dir: &[&str]) -> bool {
        // The literal prefix must be a prefix of the directory path.
        if !self.literal_prefix.is_empty() {
            if dir.len() < self.literal_prefix.len() {
                return false;
            }
            for (i, want) in self.literal_prefix.iter().enumerate() {
                if dir[i] != want {
                    return false;
                }
            }
        }
        match self.max_depth {
            // A pattern without a globstar can only match paths of exactly
            // `max_depth` segments; anything deeper is impossible.
            Some(max) => dir.len() + 1 <= max,
            None => true,
        }
    }
}

/// The literal text of a segment, when it has no wildcards.
fn literal_of(tokens: &[Token]) -> Option<String> {
    let mut out = String::new();
    for token in tokens {
        match token {
            Token::Char(c) => out.push(*c),
            _ => return None,
        }
    }
    Some(out)
}

/// The leading literal part of a segment.
fn leading_literal(tokens: &[Token]) -> Option<String> {
    let mut out = String::new();
    for token in tokens {
        match token {
            Token::Char(c) => out.push(*c),
            _ => break,
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// The trailing literal part of a segment.
fn trailing_literal(tokens: &[Token]) -> Option<String> {
    let mut out: Vec<char> = Vec::new();
    for token in tokens.iter().rev() {
        match token {
            Token::Char(c) => out.push(*c),
            _ => break,
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out.into_iter().rev().collect())
    }
}

/// Match pattern segments against path segments.
fn match_segments(pat: &[Segment], path: &[&str], trailing_needs_slash: bool) -> bool {
    if pat.is_empty() {
        return path.is_empty();
    }
    match &pat[0] {
        Segment::GlobStar => {
            if pat.len() == 1 {
                // Trailing `**`.
                //
                // `picomatch` compiles a trailing `/**` to "(slash + anything) or
                // nothing" - but only when the preceding segment does not end
                // in a wildcard. `a/**` therefore matches `a` while `a*/**`
                // requires a trailing slash.
                let min = if trailing_needs_slash { 1 } else { 0 };
                for i in min..=path.len() {
                    if match_segments(&pat[1..], &path[i..], trailing_needs_slash) {
                        return true;
                    }
                }
                false
            } else {
                // A globstar in the middle may absorb zero segments.
                for i in 0..=path.len() {
                    if match_segments(&pat[1..], &path[i..], trailing_needs_slash) {
                        return true;
                    }
                }
                false
            }
        }
        Segment::Pattern(tokens) => {
            if path.is_empty() {
                return false;
            }
            if !match_tokens(tokens, path[0]) {
                return false;
            }
            match_segments(&pat[1..], &path[1..], trailing_needs_slash)
        }
    }
}

/// Match a single path segment against a compiled token list.
///
/// The text is matched by characters, not by bytes: a `?` matches one
/// character (and not one byte of a multi byte character), and a literal
/// character in the pattern is compared with its UTF-8 encoding.
fn match_tokens(tokens: &[Token], text: &str) -> bool {
    // Iterative backtracking matcher.
    fn go(tokens: &[Token], text: &str, ti: usize, xi: usize) -> bool {
        if ti == tokens.len() {
            return xi == text.len();
        }
        match &tokens[ti] {
            Token::Char(c) => {
                if text[xi..].starts_with(*c) {
                    go(tokens, text, ti + 1, xi + c.len_utf8())
                } else {
                    false
                }
            }
            Token::Any => {
                match text[xi..].chars().next() {
                    Some(c) => go(tokens, text, ti + 1, xi + c.len_utf8()),
                    None => false,
                }
            }
            Token::Star => {
                // Try every possible consumption.
                let mut k = xi;
                loop {
                    if go(tokens, text, ti + 1, k) {
                        return true;
                    }
                    if k >= text.len() {
                        return false;
                    }
                    k += text[k..].chars().next().map_or(1, char::len_utf8);
                }
            }
            Token::Class { negated, items } => match text[xi..].chars().next() {
                Some(c) if class_matches(items, c, *negated) => {
                    go(tokens, text, ti + 1, xi + c.len_utf8())
                }
                _ => false,
            },
        }
    }
    go(tokens, text, 0, 0)
}

fn class_matches(items: &[ClassItem], c: char, negated: bool) -> bool {
    let mut hit = false;
    for item in items {
        match item {
            ClassItem::Char(x) => {
                if *x == c {
                    hit = true;
                    break;
                }
            }
            ClassItem::Range(a, z) => {
                if c >= *a && c <= *z {
                    hit = true;
                    break;
                }
            }
        }
    }
    hit != negated
}

/// Compile one path segment into tokens.
fn compile_tokens(raw: &str) -> Vec<Token> {
    let chars: Vec<char> = raw.chars().collect();
    let mut tokens = Vec::with_capacity(chars.len());
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\\' => {
                if i + 1 < chars.len() {
                    tokens.push(Token::Char(chars[i + 1]));
                    i += 2;
                } else {
                    tokens.push(Token::Char('\\'));
                    i += 1;
                }
            }
            '*' => {
                // Collapse runs of `*` inside a segment (they behave like one).
                while i < chars.len() && chars[i] == '*' {
                    i += 1;
                }
                tokens.push(Token::Star);
            }
            '?' => {
                tokens.push(Token::Any);
                i += 1;
            }
            '[' => {
                if let Some((tok, consumed)) = compile_class(&chars, i) {
                    tokens.push(tok);
                    i += consumed;
                } else {
                    tokens.push(Token::Char('['));
                    i += 1;
                }
            }
            c => {
                tokens.push(Token::Char(c));
                i += 1;
            }
        }
    }
    tokens
}

/// Compile a `[...]` character class starting at `start`.
fn compile_class(chars: &[char], start: usize) -> Option<(Token, usize)> {
    let mut i = start + 1;
    let mut negated = false;
    if i < chars.len() && (chars[i] == '!' || chars[i] == '^') {
        negated = true;
        i += 1;
    }
    let mut items = Vec::new();
    let mut closed = false;
    while i < chars.len() {
        if chars[i] == ']' && !items.is_empty() {
            closed = true;
            i += 1;
            break;
        }
        if i + 2 < chars.len() && chars[i + 1] == '-' && chars[i + 2] != ']' {
            items.push(ClassItem::Range(chars[i], chars[i + 2]));
            i += 3;
        } else {
            items.push(ClassItem::Char(chars[i]));
            i += 1;
        }
    }
    if !closed {
        return None;
    }
    Some((Token::Class { negated, items }, i - start))
}

/// Expand `{a,b}` brace groups into a list of concrete patterns.
pub fn expand_braces(pattern: &str) -> Vec<String> {
    let chars: Vec<char> = pattern.chars().collect();
    let mut depth = 0usize;
    let mut open = None;
    let mut i = 0usize;
    while i < chars.len() {
        match chars[i] {
            '\\' => i += 1,
            '{' => {
                if depth == 0 {
                    open = Some(i);
                }
                depth += 1;
            }
            '}' => {
                if depth > 0 {
                    depth -= 1;
                    if depth == 0 {
                        let start = open.unwrap();
                        let inner: String = chars[start + 1..i].iter().collect();
                        let prefix: String = chars[..start].iter().collect();
                        let suffix: String = chars[i + 1..].iter().collect();
                        let mut out = Vec::new();
                        for alt in split_alternatives(&inner) {
                            let candidate = format!("{prefix}{alt}{suffix}");
                            if candidate.contains('{') {
                                out.extend(expand_braces(&candidate));
                            } else {
                                out.push(candidate);
                            }
                        }
                        return out;
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }
    vec![pattern.to_string()]
}

/// Split `a,b,c` on top-level commas.
fn split_alternatives(inner: &str) -> Vec<String> {
    let chars: Vec<char> = inner.chars().collect();
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut cur = String::new();
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\\' => {
                cur.push(c);
                if i + 1 < chars.len() {
                    cur.push(chars[i + 1]);
                    i += 2;
                    continue;
                }
            }
            '{' => {
                depth += 1;
                cur.push(c);
            }
            '}' => {
                depth -= 1;
                cur.push(c);
            }
            ',' if depth == 0 => {
                parts.push(std::mem::take(&mut cur));
            }
            _ => cur.push(c),
        }
        i += 1;
    }
    parts.push(cur);
    parts
}

/// Repomix's `escapeGlobPattern`: escape backslashes and bracket characters.
pub fn escape_glob_pattern(pattern: &str) -> String {
    let mut out = String::with_capacity(pattern.len());
    for c in pattern.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '(' | ')' | '[' | ']' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

/// Repomix's `normalizeGlobPattern` (applied to ignore patterns only).
pub fn normalize_glob_pattern(pattern: &str) -> String {
    if pattern.ends_with('/') && !pattern.ends_with("**/") {
        return pattern[..pattern.len() - 1].to_string();
    }
    if pattern.starts_with("**/") && !pattern.contains("/**") {
        return format!("{pattern}/**");
    }
    pattern.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matches(pattern: &str, path: &str) -> bool {
        PatternSet {
            patterns: vec![Pattern::compile(pattern).unwrap()],
        }
        .is_match(path)
    }

    #[test]
    fn globstar_semantics() {
        assert!(matches("**/*", "foo.txt"));
        assert!(matches("**/*", "a/b/c.txt"));
        assert!(matches("src/**/*.rs", "src/main.rs"));
        assert!(matches("src/**/*.rs", "src/a/b/main.rs"));
        assert!(!matches("src/**/*.rs", "lib/main.rs"));
        assert!(!matches("*.md", "docs/readme.md"));
        assert!(matches("*.md", "readme.md"));
    }

    #[test]
    fn default_ignore_patterns() {
        assert!(matches("target/**", "target/debug/x"));
        assert!(matches("**/node_modules/**", "node_modules/x"));
        assert!(matches("**/node_modules/**", "a/node_modules/x"));
        assert!(!matches("target/**", "a/target/x"));
        assert!(matches("**/*.log", "a/b.log"));
        assert!(matches("**/*.log", "b.log"));
    }

    #[test]
    fn brace_expansion() {
        assert_eq!(expand_braces("a{b,c}d"), vec!["abd", "acd"]);
        assert_eq!(expand_braces("plain"), vec!["plain"]);
    }

    #[test]
    fn escaping_helpers() {
        assert_eq!(escape_glob_pattern("a(b)"), "a\\(b\\)");
        assert_eq!(normalize_glob_pattern("docs/"), "docs");
        assert_eq!(normalize_glob_pattern("**/dist"), "**/dist/**");
    }

    /// A `?` matches a character and not a byte.
    ///
    /// The reference implementation matches with `picomatch`, which works on
    /// characters - so a pattern like `???.ts` matches a file name with
    /// three characters, no matter how many bytes they need.
    #[test]
    fn matches_characters_and_not_bytes() {
        assert!(matches("???.ts", "abc.ts"));
        assert!(matches("???.ts", "åäö.ts"));
        assert!(!matches("???.ts", "ab.ts"));
        assert!(!matches("???.ts", "abcd.ts"));
        // A literal character in the pattern.
        assert!(matches("åäö.ts", "åäö.ts"));
        assert!(!matches("åäö.ts", "abc.ts"));
        assert!(matches("*åäö*", "xåäöy"));
        // A character class.
        assert!(matches("[åäö].ts", "å.ts"));
        assert!(!matches("[åäö].ts", "a.ts"));
    }

    /// The cheap conditions must not change the result of the matcher.
    ///
    /// `may_match` is only allowed to reject a path that the exact matcher
    /// would reject as well. This test checks that for every pattern of the
    /// default ignore list and a list of paths.
    #[test]
    fn the_cheap_conditions_never_change_the_result() {
        let paths = [
            "a.txt",
            "a/b.txt",
            "a/b/c.txt",
            "target",
            "target/debug/x",
            "a/target/debug/x",
            "node_modules",
            "node_modules/x",
            "a/node_modules/x",
            "a/node_modules/b/c",
            ".git",
            ".git/config",
            "a/.git/config",
            "package-lock.json",
            "a/package-lock.json",
            "a/b/package-lock.json",
            "x.log",
            "a/x.log",
            "a/b/x.log",
            "a/b/x.log.1",
            "a/b/c.log",
            "foo.pyc",
            "a/foo.pyc",
            "a/foo.pyo",
            "a/foo.py",
            "dist",
            "dist/x",
            "a/dist/x",
            "build/Release/x",
            "a/build/Release/x",
            "npm-debug.log",
            "a/yarn-error.log",
            "repomix-output.xml",
            "a/repomix-output.xml",
            "Cargo.lock",
            "a/Cargo.lock",
            "a/b/Cargo.lock.orig",
            "a/b/Cargo.toml.orig",
            "src/main.rs",
            "src\\main.rs",
            "",
            "a//b",
            ".",
            "..",
            "a/..",
        ];
        let patterns: Vec<String> = crate::core::search::DEFAULT_IGNORE_LIST
            .iter()
            .map(|p| crate::util::glob::normalize_glob_pattern(p))
            .collect();
        for pattern in &patterns {
            let compiled = Pattern::compile(pattern)
                .unwrap_or_else(|| panic!("the pattern {pattern:?} does not compile"));
            for path in &paths {
                let segments = crate::util::path::split_segments(path);
                let exact = compiled.matches(&segments);
                let fast = compiled.may_match(&segments) && exact;
                assert_eq!(
                    exact, fast,
                    "the cheap conditions changed the result for {pattern:?} and {path:?}"
                );
            }
        }
    }
}
