//! A small, dependency-free glob matcher that reproduces the semantics of
//! the `minimatch` package used by Repomix (via `globby` / `fast-glob`).
//!
//! Supported syntax:
//!
//! * `*` — matches any run of characters inside a single path segment
//! * `?` — matches exactly one character inside a single path segment
//! * `**` — as a whole segment, matches zero or more path segments
//! * `[abc]`, `[a-z]`, `[!abc]` — character classes
//! * `{a,b}` — brace expansion
//! * `\x` — literal escape
//! * `!pattern` — negation (handled by the caller, see [`PatternSet`])

/// A single path segment pattern.
#[derive(Debug, Clone)]
enum Segment {
    /// Literal text, possibly containing `*`/`?`/classes.
    Pattern(Vec<Token>),
    /// `**` — matches zero or more segments.
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
        self.patterns.iter().any(|p| p.matches(&segments))
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
        let mut max_depth = Some(0usize);

        for raw in &raw_segments {
            if *raw == "**" {
                segments.push(Segment::GlobStar);
                has_globstar = true;
            } else {
                segments.push(Segment::Pattern(compile_tokens(raw)));
                min_depth += 1;
            }
        }

        if has_globstar {
            max_depth = None;
        } else {
            max_depth = Some(min_depth);
        }

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

        Some(Pattern {
            segments,
            min_depth,
            max_depth,
            literal_prefix,
            has_globstar,
        })
    }

    pub fn matches(&self, path_segments: &[&str]) -> bool {
        match_segments(&self.segments, path_segments)
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
        match_segments(prefix, dir)
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

/// Match pattern segments against path segments.
fn match_segments(pat: &[Segment], path: &[&str]) -> bool {
    if pat.is_empty() {
        return path.is_empty();
    }
    match &pat[0] {
        Segment::GlobStar => {
            // `**` may absorb zero or more path segments.
            // Guard against pathological patterns with a simple recursion.
            for i in 0..=path.len() {
                if match_segments(&pat[1..], &path[i..]) {
                    return true;
                }
            }
            false
        }
        Segment::Pattern(tokens) => {
            if path.is_empty() {
                return false;
            }
            if !match_tokens(tokens, path[0].as_bytes()) {
                return false;
            }
            match_segments(&pat[1..], &path[1..])
        }
    }
}

/// Match a single path segment against a compiled token list.
fn match_tokens(tokens: &[Token], text: &[u8]) -> bool {
    // Iterative backtracking matcher.
    fn go(tokens: &[Token], text: &[u8], ti: usize, xi: usize) -> bool {
        if ti == tokens.len() {
            return xi == text.len();
        }
        match &tokens[ti] {
            Token::Char(c) => {
                if xi < text.len() && text[xi] == *c as u8 {
                    go(tokens, text, ti + 1, xi + 1)
                } else {
                    false
                }
            }
            Token::Any => {
                if xi < text.len() {
                    go(tokens, text, ti + 1, xi + 1)
                } else {
                    false
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
                    k += 1;
                }
            }
            Token::Class { negated, items } => {
                if xi < text.len() && class_matches(items, text[xi], *negated) {
                    go(tokens, text, ti + 1, xi + 1)
                } else {
                    false
                }
            }
        }
    }
    go(tokens, text, 0, 0)
}

fn class_matches(items: &[ClassItem], b: u8, negated: bool) -> bool {
    let c = b as char;
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
}
