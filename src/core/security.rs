//! Secret detection.
//!
//! A re-implementation of the `secretlint` "recommend" preset that Repomix
//! uses. Files that match are excluded from the packed output.
//!
//! The regular expressions are ported from the upstream preset; the
//! look-around assertions are evaluated manually because the `regex` crate
//! does not support them.

use std::sync::OnceLock;

use regex::Regex;

/// How the characters surrounding a match are validated.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Head {
    /// No constraint.
    Any,
    /// `(?<!\p{L})` — the preceding character must not be a letter.
    NotLetter,
}

/// How the characters following a match are validated.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tail {
    /// No constraint.
    Any,
    /// `(?![A-Za-z0-9_])`
    NotWord,
    /// `(?![a-zA-Z0-9])`
    NotAlnum,
    /// `(?![a-zA-Z])`
    NotAlpha,
    /// `(?![A-Za-z0-9+/=])`
    NotBase64,
}

impl Tail {
    fn allows(self, c: char) -> bool {
        match self {
            Tail::Any => true,
            Tail::NotWord => !(c.is_ascii_alphanumeric() || c == '_'),
            Tail::NotAlnum => !c.is_ascii_alphanumeric(),
            Tail::NotAlpha => !c.is_ascii_alphabetic(),
            Tail::NotBase64 => {
                !(c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=')
            }
        }
    }
}

/// The kind of extra validation a rule needs.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// Every match is an issue.
    Plain,
    /// AWS secret access key: the allow list applies to the capture group.
    AwsSecret,
    /// `scheme://user:password@host` style credentials.
    BasicAuth,
    /// Database connection strings with credentials.
    DbConn,
    /// PEM encoded private keys.
    PrivateKey,
    /// SendGrid keys have a fixed length.
    SendGrid,
}

struct Rule {
    name: &'static str,
    /// The raw regex pattern. The regex itself is compiled lazily, because
    /// compiling all 35 rules costs ~120 ms and is paid by every single run.
    pattern: &'static str,
    /// Literals that must be present in a file for this rule to be able to
    /// match at all. The rule is only compiled and evaluated when at least
    /// one of them occurs in the content.
    ///
    /// Every entry here has to be a *necessary* substring of the pattern: if
    /// the literal is missing, the regex provably cannot match.
    literals: &'static [&'static str],
    head: Head,
    tail: Tail,
    kind: Kind,
    /// The lazily compiled regex. A failed compilation is cached as well, so
    /// a broken pattern is not retried for every file.
    re: OnceLock<Option<Regex>>,
}

impl Rule {
    /// The compiled regex, compiled on first use.
    fn regex(&self) -> Option<&Regex> {
        self.re
            .get_or_init(|| Regex::new(self.pattern).ok())
            .as_ref()
    }
}

/// A multi literal prefilter that decides which rules can possibly match.
struct Prefilter {
    ac: aho_corasick::AhoCorasick,
    /// For every literal, the bitmask of the rules it activates.
    ///
    /// A literal can belong to more than one rule (e.g. `ghs_` is shared by
    /// two GitHub rules), so this is a mask and not a single rule index.
    literal_to_mask: Vec<u64>,
}

impl Prefilter {
    /// The bitmask of rules that may match the given content.
    fn candidate_rules(&self, content: &str) -> u64 {
        let mut mask = 0u64;
        for m in self.ac.find_iter(content) {
            mask |= self.literal_to_mask[m.pattern().as_usize()];
        }
        mask
    }
}

/// The prefilter over all rule literals.
fn prefilter() -> Option<&'static Prefilter> {
    static PREFILTER: OnceLock<Option<Prefilter>> = OnceLock::new();
    PREFILTER
        .get_or_init(|| {
            // A literal may activate several rules, and aho-corasick does not
            // accept duplicate patterns - so the literals are de-duplicated
            // and every unique literal carries the mask of all its rules.
            let mut masks: std::collections::HashMap<&'static str, u64> =
                std::collections::HashMap::new();
            for (index, rule) in rules().iter().enumerate() {
                for literal in rule.literals {
                    *masks.entry(literal).or_insert(0) |= 1u64 << index;
                }
            }
            let mut patterns: Vec<&'static str> = Vec::with_capacity(masks.len());
            let mut literal_to_mask: Vec<u64> = Vec::with_capacity(masks.len());
            for (literal, mask) in masks {
                patterns.push(literal);
                literal_to_mask.push(mask);
            }
            let ac = aho_corasick::AhoCorasickBuilder::new()
                .match_kind(aho_corasick::MatchKind::Standard)
                .build(&patterns)
                .ok()?;
            Some(Prefilter {
                ac,
                literal_to_mask,
            })
        })
        .as_ref()
}

/// Placeholders that suppress the basic-auth rule.
const BASIC_AUTH_IGNORED: &[&str] = &[
    "localhost",
    "127.0.0.1",
    "password",
    "mypassword",
    "myusername",
    "{password}",
    "${password}",
    "{{password}}",
    "{username}",
    "${username}",
    "{{username}}",
    "YOUR_PASSWORD",
    "YOUR_USERNAME",
    "REPLACE_WITH_PASSWORD",
    "REPLACE_WITH_USERNAME",
];

/// Placeholders that suppress the database-connection-string rule.
const DB_IGNORED: &[&str] = &[
    "localhost",
    "127.0.0.1",
    "example.com",
    "example.org",
    "test.com",
    "password",
    "username",
    "secret",
    "mypassword",
    "myusername",
    "root",
    "admin",
    "user",
    "pass",
    "{password}",
    "${password}",
    "{{password}}",
    "{username}",
    "${username}",
    "{{username}}",
    "YOUR_PASSWORD",
    "YOUR_USERNAME",
    "REPLACE_WITH_PASSWORD",
];

/// AWS example keys that are ignored by default.
const AWS_IGNORED_SECRET: &[&str] = &["wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"];

/// Does the string look like a template placeholder?
fn is_variable_like(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'$' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'{' {
                    if let Some(end) = find_close(bytes, i + 2, b'}') {
                        if end - (i + 2) <= 50 {
                            return true;
                        }
                    }
                } else if i + 1 < bytes.len() && bytes[i + 1] == b'(' {
                    if let Some(end) = find_close(bytes, i + 2, b')') {
                        if end - (i + 2) <= 50 {
                            return true;
                        }
                    }
                } else {
                    let mut j = i + 1;
                    while j < bytes.len() && (bytes[j].is_ascii_uppercase() || bytes[j] == b'_') {
                        j += 1;
                    }
                    if j > i + 1 && j - (i + 1) <= 30 {
                        return true;
                    }
                }
            }
            b'%' => {
                let mut j = i + 1;
                while j < bytes.len() && (bytes[j].is_ascii_uppercase() || bytes[j] == b'_') {
                    j += 1;
                }
                if j > i + 1 && j - (i + 1) <= 30 && j < bytes.len() && bytes[j] == b'%' {
                    return true;
                }
            }
            b'{' => {
                if let Some(end) = find_close(bytes, i + 1, b'}') {
                    if end - (i + 1) <= 50 {
                        return true;
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }
    false
}

fn find_close(bytes: &[u8], from: usize, close: u8) -> Option<usize> {
    let mut i = from;
    while i < bytes.len() {
        if bytes[i] == close {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Is the username part of a URL a real credential?
fn is_likely_real_username(username: &str) -> bool {
    if username.len() < 2 {
        return false;
    }
    if is_variable_like(username) {
        return false;
    }
    let lower = username.to_ascii_lowercase();
    !BASIC_AUTH_IGNORED
        .iter()
        .any(|p| lower.contains(&p.to_ascii_lowercase()))
}

/// Is the password part of a URL a real credential?
fn is_likely_real_password(password: &str) -> bool {
    if password.len() < 4 {
        return false;
    }
    if is_variable_like(password) {
        return false;
    }
    let lower = password.to_ascii_lowercase();
    if BASIC_AUTH_IGNORED
        .iter()
        .any(|p| lower.contains(&p.to_ascii_lowercase()))
    {
        return false;
    }
    let bytes = password.as_bytes();
    if bytes.len() >= 4 && bytes.iter().all(|b| *b == bytes[0]) {
        return false;
    }
    true
}

/// Are the credentials of a connection string likely real?
fn is_likely_real_credential(username: &str, password: &str) -> bool {
    if username == password {
        return false;
    }
    if password.len() < 4 {
        return false;
    }
    if is_variable_like(password) {
        return false;
    }
    let lower = password.to_ascii_lowercase();
    if DB_IGNORED
        .iter()
        .any(|p| lower == p.to_ascii_lowercase())
    {
        return false;
    }
    let bytes = password.as_bytes();
    if bytes.len() >= 4 && bytes.iter().all(|b| *b == bytes[0]) {
        return false;
    }
    true
}

/// Validate the base64 payload of a PEM encoded private key.
fn is_real_private_key_content(raw: &str) -> bool {
    let content: String = raw
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '\\')
        .collect();
    if content.len() < 100 {
        return false;
    }
    if !content
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/' || b == b'=')
    {
        return false;
    }
    content.starts_with("MI") || content.starts_with("b3BlbnNzaC1rZXktdjE")
}

fn rules() -> &'static Vec<Rule> {
    static RULES: OnceLock<Vec<Rule>> = OnceLock::new();
    RULES.get_or_init(|| {
        let specs: &[(&str, &str, &[&str], Head, Tail, Kind)] = &[
            // AWS secret access key (the access key id rule is disabled by
            // default upstream).
            (
                "aws",
                r#"["']?(?:AWS|aws|Aws)?_?(?:SECRET|secret|Secret)_?(?:ACCESS|access|Access)_?(?:KEY|key|Key)["']?\s*(?::|=>|=)\s*["']?([A-Za-z0-9/\+=]{40})["']?\b"#,
                &["SECRET", "secret", "Secret"],
                Head::Any,
                Tail::Any,
                Kind::AwsSecret,
            ),
            // npm access token
            (
                "npm",
                r"npm_[A-Za-z0-9_]{36}",
                &["npm_"],
                Head::NotLetter,
                Tail::NotWord,
                Kind::Plain,
            ),
            // npm OAuth token (only valid inside package.json)
            (
                "npm",
                r"https?://(.{1,256}):x-oauth-basic@github\.com/",
                &["x-oauth-basic@github.com/"],
                Head::Any,
                Tail::Any,
                Kind::Plain,
            ),
            // npm auth token (only valid inside .npmrc)
            (
                "npm",
                r"_authToken=([^$].*)",
                &["_authToken="],
                Head::Any,
                Tail::Any,
                Kind::Plain,
            ),
            // Slack
            (
                "slack",
                r"\b(?:xoxb|xoxp|xapp|xoxa|xoxo|xoxr)-(?:\d-)?(?:[a-zA-Z0-9]{1,40}-)+[a-zA-Z0-9]{1,40}\b",
                &["xoxb-", "xoxp-", "xapp-", "xoxa-", "xoxo-", "xoxr-"],
                Head::Any,
                Tail::Any,
                Kind::Plain,
            ),
            (
                "slack",
                r"https://hooks\.slack\.com/services/T[a-zA-Z0-9]{1,40}/B[a-zA-Z0-9]{1,40}/[a-zA-Z0-9]{1,40}",
                &["hooks.slack.com/services/T"],
                Head::Any,
                Tail::Any,
                Kind::Plain,
            ),
            // Basic auth credentials
            (
                "basicauth",
                r"(?:https?|ftp|ftps)://[-a-zA-Z0-9_]{1,256}:[-a-zA-Z0-9_]{1,256}@[-a-zA-Z0-9%._+~#=]{1,256}\.[a-zA-Z0-9()]{1,6}\b",
                &["://"],
                Head::Any,
                Tail::Any,
                Kind::BasicAuth,
            ),
            // OpenAI
            (
                "openai",
                r"sk-(?:proj|svcacct|admin)-(?:[A-Za-z0-9_-]{74}|[A-Za-z0-9_-]{58})T3BlbkFJ(?:[A-Za-z0-9_-]{74}|[A-Za-z0-9_-]{58})\b|sk-[a-zA-Z0-9]{20}T3BlbkFJ[a-zA-Z0-9]{20}",
                &["T3BlbkFJ"],
                Head::Any,
                Tail::Any,
                Kind::Plain,
            ),
            // Anthropic
            (
                "anthropic",
                r"sk-ant-api0\d-[A-Za-z0-9_-]{90,128}AA",
                &["sk-ant-api0"],
                Head::NotLetter,
                Tail::NotWord,
                Kind::Plain,
            ),
            // Groq
            (
                "groq",
                r"gsk_[a-zA-Z0-9]{52}",
                &["gsk_"],
                Head::NotLetter,
                Tail::NotAlnum,
                Kind::Plain,
            ),
            // Hugging Face
            (
                "huggingface",
                r"hf_[a-zA-Z]{34}",
                &["hf_"],
                Head::NotLetter,
                Tail::NotAlpha,
                Kind::Plain,
            ),
            // Linear
            (
                "linear",
                r"lin_api_[a-zA-Z0-9_]{32,128}",
                &["lin_api_"],
                Head::NotLetter,
                Tail::NotWord,
                Kind::Plain,
            ),
            // Notion
            (
                "notion",
                r"ntn_[0-9]{11}[A-Za-z0-9]{35}",
                &["ntn_"],
                Head::NotLetter,
                Tail::NotAlnum,
                Kind::Plain,
            ),
            // Private keys
            (
                "privatekey",
                r"-----BEGIN[ ]?(?:(?:RSA|DSA|EC|OPENSSH|PGP) )?PRIVATE KEY(?: BLOCK)?-----\n?([A-Za-z0-9+/=\s\\]{100,10000})\n?-----END[ ]?(?:(?:RSA|DSA|EC|OPENSSH|PGP) )?PRIVATE KEY(?: BLOCK)?-----",
                &["-----BEGIN"],
                Head::Any,
                Tail::Any,
                Kind::PrivateKey,
            ),
            // SendGrid
            (
                "sendgrid",
                r"SG\.\w{1,128}\.\w{1,128}([-_]?)\w{1,128}",
                &["SG."],
                Head::NotLetter,
                Tail::NotWord,
                Kind::SendGrid,
            ),
            // Shopify
            (
                "shopify",
                r"(?:shppa|shpca|shpat|shpss)_[a-zA-Z0-9]{32,64}",
                &["shppa_", "shpca_", "shpat_", "shpss_"],
                Head::NotLetter,
                Tail::NotAlnum,
                Kind::Plain,
            ),
            // Stripe
            (
                "stripe",
                r"(?:sk_live|sk_test|rk_live|rk_test)_[a-zA-Z0-9]{24,99}",
                &["sk_live_", "sk_test_", "rk_live_", "rk_test_"],
                Head::NotLetter,
                Tail::NotAlnum,
                Kind::Plain,
            ),
            // GitHub
            (
                "github",
                r"(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9_]{36}",
                &["ghp_", "gho_", "ghu_", "ghs_", "ghr_"],
                Head::NotLetter,
                Tail::NotWord,
                Kind::Plain,
            ),
            (
                "github",
                r"github_pat_[A-Za-z0-9_]{82}",
                &["github_pat_"],
                Head::NotLetter,
                Tail::NotWord,
                Kind::Plain,
            ),
            (
                "github",
                r"ghs_[0-9]{1,20}_[A-Za-z0-9_-]{1,1000}\.[A-Za-z0-9_-]{1,10000}\.[A-Za-z0-9_-]{1,1000}",
                &["ghs_"],
                Head::NotLetter,
                Tail::NotWord,
                Kind::Plain,
            ),
            // GitLab
            (
                "gitlab",
                r"glpat-[A-Za-z0-9_-]{20,128}",
                &["glpat-"],
                Head::NotLetter,
                Tail::NotWord,
                Kind::Plain,
            ),
            // Grafana
            (
                "grafana",
                r"glc_[A-Za-z0-9+/]{32,400}={0,2}",
                &["glc_"],
                Head::NotLetter,
                Tail::NotBase64,
                Kind::Plain,
            ),
            (
                "grafana",
                r"glsa_[a-zA-Z0-9]{32}_[A-Fa-f0-9]{8}",
                &["glsa_"],
                Head::NotLetter,
                Tail::NotAlnum,
                Kind::Plain,
            ),
            // 1Password
            (
                "1password",
                r"ops_ey[A-Za-z0-9+/=]{100,1280}fQ={0,2}",
                &["ops_ey"],
                Head::NotLetter,
                Tail::NotBase64,
                Kind::Plain,
            ),
            // Database connection strings
            (
                "database-connection-string",
                r"mongodb(?:\+srv)?://(?:([^:/\s]{1,100}):([^@/\s]{1,200})@)?([^/\s?]{1,100})",
                &["mongodb"],
                Head::Any,
                Tail::Any,
                Kind::DbConn,
            ),
            (
                "database-connection-string",
                r"(?:jdbc:)?mysql(?:x)?://(?:([^:/\s]{1,100}):([^@/\s]{1,200})@)?([^/\s?]{1,100})",
                &["mysql"],
                Head::Any,
                Tail::Any,
                Kind::DbConn,
            ),
            (
                "database-connection-string",
                r"postgres(?:ql)?://(?:([^:/\s]{1,100}):([^@/\s]{1,200})@)?([^/\s?]{1,100})",
                &["postgres"],
                Head::Any,
                Tail::Any,
                Kind::DbConn,
            ),
            // HashiCorp Vault
            (
                "hashicorp-vault",
                r"hvs\.[A-Za-z0-9_-]{90,120}",
                &["hvs."],
                Head::NotLetter,
                Tail::NotWord,
                Kind::Plain,
            ),
            (
                "hashicorp-vault",
                r"hvb\.[A-Za-z0-9_-]{138,300}",
                &["hvb."],
                Head::NotLetter,
                Tail::NotWord,
                Kind::Plain,
            ),
            (
                "hashicorp-vault",
                r"hvr\.[A-Za-z0-9_-]{90,120}",
                &["hvr."],
                Head::NotLetter,
                Tail::NotWord,
                Kind::Plain,
            ),
            // Vercel
            (
                "vercel",
                r"(?:vcp|vci|vca|vcr|vck)_[A-Za-z0-9]{20,60}",
                &["vcp_", "vci_", "vca_", "vcr_", "vck_"],
                Head::NotLetter,
                Tail::NotAlnum,
                Kind::Plain,
            ),
            // Databricks
            (
                "databricks",
                r"dapi[A-Fa-f0-9]{32}(?:-[0-9])?",
                &["dapi"],
                Head::NotLetter,
                Tail::NotAlnum,
                Kind::Plain,
            ),
            // Docker
            (
                "docker",
                r"dckr_pat_[a-zA-Z0-9_-]{27}",
                &["dckr_pat_"],
                Head::NotLetter,
                Tail::NotWord,
                Kind::Plain,
            ),
            // Figma
            (
                "figma",
                r"figd_[A-Za-z0-9_-]{40,200}",
                &["figd_"],
                Head::NotLetter,
                Tail::NotWord,
                Kind::Plain,
            ),
            // Cloudflare
            (
                "cloudflare",
                r"(?:cfk|cfut|cfat)_[A-Za-z0-9]{40}[0-9a-f]{8}",
                &["cfk_", "cfut_", "cfat_"],
                Head::NotLetter,
                Tail::NotAlnum,
                Kind::Plain,
            ),
            // Tailscale
            (
                "tailscale",
                r"tskey-[a-z]{2,20}-[0-9A-Za-z_]{8,40}-[0-9A-Za-z_]{16,60}",
                &["tskey-"],
                Head::NotLetter,
                Tail::NotWord,
                Kind::Plain,
            ),
        ];
        specs
            .iter()
            .map(|(name, pattern, literals, head, tail, kind)| Rule {
                name,
                pattern,
                literals,
                head: *head,
                tail: *tail,
                kind: *kind,
                re: OnceLock::new(),
            })
            .collect()
    })
}

/// The raw patterns of every rule, in evaluation order.
///
/// Exposed for diagnostics: the rules are compiled eagerly, and the total
/// compile time is a fixed cost paid by every single run.
pub fn rule_patterns() -> Vec<(&'static str, &'static str)> {
    rules().iter().map(|r| (r.name, r.pattern)).collect()
}

/// The result of scanning a single file.
#[derive(Debug, Clone)]
pub struct SecurityResult {
    pub file_path: String,
    pub messages: Vec<String>,
}

/// Iterate over the matches of a rule, applying the look-around guards.
fn find_guarded<'t>(
    re: &Regex,
    content: &'t str,
    head: Head,
    tail: Tail,
) -> Vec<regex::Match<'t>> {
    let mut out = Vec::new();
    let mut start = 0usize;
    while start <= content.len() {
        let Some(m) = re.find_at(content, start) else {
            break;
        };
        let mut next = m.end();
        // Head guard.
        let head_ok = match head {
            Head::Any => true,
            Head::NotLetter => {
                m.start() == 0 || !content[..m.start()].chars().next_back().unwrap().is_alphabetic()
            }
        };
        if !head_ok {
            next = m.start() + 1;
        } else {
            // Tail guard.
            let tail_ok = match content[m.end()..].chars().next() {
                None => true,
                Some(c) => tail.allows(c),
            };
            if tail_ok {
                out.push(m);
            } else {
                next = m.end();
            }
        }
        if next <= m.start() {
            next = m.start() + 1;
        }
        // The next search has to start at a character boundary, otherwise
        // `find_at` panics.
        while next < content.len() && !content.is_char_boundary(next) {
            next += 1;
        }
        start = next;
        if out.len() > 10_000 {
            break;
        }
    }
    out
}

/// Scan a piece of text for secrets. Returns the number of issues found.
pub fn count_issues(content: &str) -> usize {
    scan(content, "file.txt").len()
}

/// Scan a file and describe the issues that were found.
pub fn scan_file(file_path: &str, content: &str) -> Option<SecurityResult> {
    let messages = scan(content, file_path);
    if messages.is_empty() {
        None
    } else {
        Some(SecurityResult {
            file_path: file_path.to_string(),
            messages,
        })
    }
}

/// Run all rules over the content.
fn scan(content: &str, file_path: &str) -> Vec<String> {
    let mut messages = Vec::new();
    let base = file_path.rsplit(['/', '\\']).next().unwrap_or(file_path);
    let lower_base = base.to_ascii_lowercase();

    // A single multi literal pass decides which rules can possibly match.
    // Rules that cannot match are never compiled and never evaluated, which
    // removes both the ~120 ms of eager regex compilation and almost all of
    // the scanning work.
    let Some(prefilter) = prefilter() else {
        return messages;
    };
    let candidates = prefilter.candidate_rules(content);
    if candidates == 0 {
        return messages;
    }

    for (index, rule) in rules().iter().enumerate() {
        if candidates & (1u64 << index) == 0 {
            continue;
        }
        let Some(re) = rule.regex() else {
            continue;
        };
        match rule.kind {
            Kind::Plain => {
                // The npm OAuth / auth token rules only apply to specific files.
                if rule.name == "npm" {
                    if rule.pattern.contains("x-oauth-basic")
                        && !(lower_base == "package.json" || lower_base == "package-lock.json")
                    {
                        continue;
                    }
                    if rule.pattern.contains("_authToken") && lower_base != ".npmrc" {
                        continue;
                    }
                }
                for m in find_guarded(re, content, rule.head, rule.tail) {
                    messages.push(format!(
                        "Found {} pattern in {}: {}",
                        rule.name,
                        file_path,
                        truncate(m.as_str())
                    ));
                }
            }
            Kind::AwsSecret => {
                for caps in re.captures_iter(content) {
                    let key = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                    if AWS_IGNORED_SECRET.contains(&key) {
                        continue;
                    }
                    messages.push(format!(
                        "Found {} pattern in {}: {}",
                        rule.name,
                        file_path,
                        truncate(key)
                    ));
                }
            }
            Kind::BasicAuth => {
                for m in find_guarded(re, content, rule.head, rule.tail) {
                    if basic_auth_is_suspicious(m.as_str()) {
                        messages.push(format!(
                            "Found {} pattern in {}: {}",
                            rule.name,
                            file_path,
                            truncate(m.as_str())
                        ));
                    }
                }
            }
            Kind::DbConn => {
                for caps in re.captures_iter(content) {
                    let username = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                    let password = caps.get(2).map(|m| m.as_str()).unwrap_or("");
                    if username.is_empty() || password.is_empty() {
                        continue;
                    }
                    if !is_likely_real_credential(username, password) {
                        continue;
                    }
                    messages.push(format!(
                        "Found {} pattern in {}: {}",
                        rule.name,
                        file_path,
                        truncate(caps.get(0).map(|m| m.as_str()).unwrap_or(""))
                    ));
                }
            }
            Kind::PrivateKey => {
                for caps in re.captures_iter(content) {
                    let payload = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                    if !is_real_private_key_content(payload) {
                        continue;
                    }
                    messages.push(format!(
                        "Found {} pattern in {}: {}",
                        rule.name,
                        file_path,
                        truncate("-----BEGIN ... PRIVATE KEY-----")
                    ));
                }
            }
            Kind::SendGrid => {
                for m in find_guarded(re, content, rule.head, rule.tail) {
                    // SendGrid keys are exactly 69 characters long.
                    if m.as_str().len() != 69 {
                        continue;
                    }
                    messages.push(format!(
                        "Found {} pattern in {}: {}",
                        rule.name,
                        file_path,
                        truncate(m.as_str())
                    ));
                }
            }
        }
    }
    messages
}

/// Is the credential part of a URL a real secret?
fn basic_auth_is_suspicious(whole: &str) -> bool {
    let Some(scheme_end) = whole.find("://") else {
        return false;
    };
    let after_scheme = &whole[scheme_end + 3..];
    let Some(at) = after_scheme.find('@') else {
        return false;
    };
    let creds = &after_scheme[..at];
    let Some(colon) = creds.find(':') else {
        return false;
    };
    let username = &creds[..colon];
    let password = &creds[colon + 1..];
    is_likely_real_username(username) && is_likely_real_password(password)
}

fn truncate(text: &str) -> String {
    let max = 60;
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let head: String = text.chars().take(max).collect();
        format!("{head}...")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_aws_examples() {
        assert_eq!(
            count_issues("AWS_SECRET_ACCESS_KEY = \"wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY\""),
            0
        );
        assert_eq!(count_issues("AKIAIOSFODNN7EXAMPLE"), 0);
    }

    #[test]
    fn detects_real_aws_secret() {
        assert!(
            count_issues("AWS_SECRET_ACCESS_KEY = \"abcdefghij0123456789ABCDEFGHIJ0123456789\"") > 0
        );
    }

    #[test]
    fn detects_private_keys() {
        let mut key = String::from("-----BEGIN RSA PRIVATE KEY-----\n");
        key.push_str(&"MI".to_string());
        key.push_str(&"A".repeat(200));
        key.push_str("\n-----END RSA PRIVATE KEY-----");
        assert!(count_issues(&key) > 0);
    }

    #[test]
    fn ignores_placeholder_credentials() {
        assert_eq!(count_issues("https://user:password@example.com/path"), 0);
        assert_eq!(count_issues("https://${USER}:${PASS}@example.com"), 0);
    }

    #[test]
    fn detects_real_credentials() {
        assert!(count_issues("https://admin:Xy9kQ2mZpL4rT7w@mycompany.io/x") > 0);
    }

    #[test]
    fn plain_code_is_clean() {
        let code = "fn main() { println!(\"hello\"); }";
        assert_eq!(count_issues(code), 0);
    }

    /// Samples that exercise every rule, plus some ordinary source code.
    fn samples() -> Vec<String> {
        let mut v: Vec<String> = Vec::new();
        v.push("AWS_SECRET_ACCESS_KEY = \"abcdefghij0123456789ABCDEFGHIJ0123456789\"".into());
        v.push(format!("npm_{}", "a".repeat(36)));
        v.push("https://user:x-oauth-basic@github.com/org/repo.git".into());
        v.push("_authToken=npm_abcdefghijklmnopqrstuvwxyz0123456789".into());
        v.push("xoxb-1234567890-1234567890-abcdefghijklmnopqrstuvwx".into());
        v.push("https://hooks.slack.com/services/T00000000/B00000000/abcdefghijklmnopqrstuvwx".into());
        v.push("https://admin:Xy9kQ2mZpL4rT7w@mycompany.io/x".into());
        v.push(format!("sk-proj-{}{}{}", "a".repeat(74), "T3BlbkFJ", "b".repeat(74)));
        v.push(format!("sk-ant-api03-{}{}", "a".repeat(100), "AA"));
        v.push(format!("gsk_{}", "a".repeat(52)));
        v.push(format!("hf_{}", "a".repeat(34)));
        v.push(format!("lin_api_{}", "a".repeat(32)));
        v.push(format!("ntn_{}{}", "1".repeat(11), "a".repeat(35)));
        v.push(format!(
            "-----BEGIN RSA PRIVATE KEY-----\nMI{}\n-----END RSA PRIVATE KEY-----",
            "A".repeat(200)
        ));
        v.push(format!("SG.{}.{}.{}", "a".repeat(20), "b".repeat(20), "c".repeat(25)));
        v.push(format!("shppa_{}", "a".repeat(32)));
        v.push(format!("sk_live_{}", "a".repeat(24)));
        v.push(format!("ghp_{}", "a".repeat(36)));
        v.push(format!("github_pat_{}", "a".repeat(82)));
        v.push(format!("ghs_{}_{}.{}.{}", "1", "a".repeat(20), "b".repeat(20), "c".repeat(20)));
        v.push(format!("glpat-{}", "a".repeat(20)));
        v.push(format!("glc_{}", "a".repeat(32)));
        v.push(format!("glsa_{}_{}", "a".repeat(32), "b".repeat(8)));
        v.push(format!("ops_ey{}fQ=", "a".repeat(100)));
        v.push("mongodb://user:Str0ngPassw0rd@host.example.com/db".into());
        v.push("mysql://user:Str0ngPassw0rd@host.example.com/db".into());
        v.push("postgres://user:Str0ngPassw0rd@host.example.com/db".into());
        v.push(format!("hvs.{}", "a".repeat(90)));
        v.push(format!("hvb.{}", "a".repeat(138)));
        v.push(format!("hvr.{}", "a".repeat(90)));
        v.push(format!("vcp_{}", "a".repeat(20)));
        v.push(format!("dapi{}", "a".repeat(32)));
        v.push(format!("dckr_pat_{}", "a".repeat(27)));
        v.push(format!("figd_{}", "a".repeat(40)));
        v.push(format!("cfk_{}{}", "a".repeat(40), "b".repeat(8)));
        v.push(format!("tskey-{}-{}-{}", "ab", "c".repeat(8), "d".repeat(16)));
        // Ordinary source code that must not trigger anything.
        v.push("fn main() { let url = \"https://example.com/path\"; }".into());
        v.push("const x = { href: 'http://a.b/c', npm: 1, hf: 2 };".into());
        v
    }

    /// The prefilter must never exclude a rule that would have matched.
    ///
    /// This is the safety property of the literal prefilter: if a rule's
    /// literal is missing from the content, the rule's regex provably cannot
    /// match. The test checks that property for every rule and every sample.
    #[test]
    fn prefilter_never_hides_a_matching_rule() {
        let pf = prefilter().expect("prefilter");
        for sample in samples() {
            let mask = pf.candidate_rules(&sample);
            for (index, rule) in rules().iter().enumerate() {
                let Some(re) = rule.regex() else {
                    continue;
                };
                if re.find(&sample).is_some() {
                    assert!(
                        mask & (1u64 << index) != 0,
                        "rule '{}' (#{}, pattern {}) matches the sample {:?} but was excluded by the prefilter",
                        rule.name,
                        index,
                        rule.pattern,
                        sample
                    );
                }
            }
        }
    }

    /// Every match of a rule has to contain one of the rule's literals.
    ///
    /// This is the property the prefilter relies on: if a literal is missing
    /// from the content, the rule cannot match. The literal does not have to
    /// be a contiguous part of the *pattern* (an alternation like
    /// `(?:xoxb|xoxp|...)` puts it together from two pieces), but it always
    /// has to be part of the matched *text*.
    #[test]
    fn every_match_contains_one_of_its_literals() {
        for sample in samples() {
            for rule in rules() {
                let Some(re) = rule.regex() else {
                    continue;
                };
                for m in re.find_iter(&sample) {
                    assert!(
                        rule.literals.iter().any(|lit| m.as_str().contains(lit)),
                        "rule '{}' matched {:?} which contains none of its literals {:?}",
                        rule.name,
                        m.as_str(),
                        rule.literals
                    );
                }
            }
        }
    }
}
