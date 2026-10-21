//! Secret detection.
//!
//! A lightweight re-implementation of the `secretlint` "recommend" preset
//! that Repomix uses. Files that match are excluded from the packed output.

use std::sync::OnceLock;

use regex::Regex;

/// A detection rule.
struct Rule {
    name: &'static str,
    re: Regex,
    kind: RuleKind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RuleKind {
    Plain,
    /// `scheme://user:password@host` style credentials.
    BasicAuth,
}

/// Values that suppress the basic-auth rule.
const BUILTIN_IGNORED_PATTERNS: &[&str] = &[
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

/// Matches `${var}`, `{{var}}`, `{var}`, `%VAR%`, `$VAR` and `$(var)`.
fn looks_like_variable(text: &str) -> bool {
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

/// Is the credential part of a URL a placeholder rather than a real secret?
fn is_ignored_credential(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    if BUILTIN_IGNORED_PATTERNS
        .iter()
        .any(|p| lower.contains(&p.to_ascii_lowercase()))
    {
        return true;
    }
    if looks_like_variable(url) {
        return true;
    }
    false
}

/// Extract `user` and `password` from a `scheme://user:pass@host` match.
fn basic_auth_is_suspicious(m: &regex::Match<'_>) -> bool {
    let whole = m.as_str();
    let Some(sep) = whole.find(':') else {
        return true;
    };
    let after_scheme = match whole.find("://") {
        Some(i) => &whole[i + 3..],
        None => whole,
    };
    let Some(at) = after_scheme.find('@') else {
        return true;
    };
    let creds = &after_scheme[..at];
    let Some(colon) = creds.find(':') else {
        return true;
    };
    let user = &creds[..colon];
    let password = &creds[colon + 1..];
    let _ = sep;

    if password.len() < 4 {
        return false;
    }
    if user.len() < 2 {
        return false;
    }
    if user == password {
        return false;
    }
    // A run of four or more identical characters is not a real password.
    let bytes = password.as_bytes();
    if bytes.len() >= 4 && bytes.iter().all(|b| *b == bytes[0]) {
        return false;
    }
    if is_ignored_credential(whole) {
        return false;
    }
    true
}

fn rules() -> &'static Vec<Rule> {
    static RULES: OnceLock<Vec<Rule>> = OnceLock::new();
    RULES.get_or_init(|| {
        let specs: &[(&str, &str, RuleKind)] = &[
            ("aws", r"\b(?:A3T[A-Z0-9]|AKIA|AGPA|AIDA|AROA|AIPA|ANPA|ANVA|ASIA)[A-Z0-9]{16}\b", RuleKind::Plain),
            ("aws", r#"(?i)(?:AWS|aws|Aws)?_?(?:SECRET|secret|Secret)_?(?:ACCESS|access|Access)_?(?:KEY|key|Key)["']?\s*(?::|=>|=)\s*["']?([A-Za-z0-9/\+=]{40})"# , RuleKind::Plain),
            ("privatekey", r"-----BEGIN\s?(?:DSA|RSA|EC|PGP|OPENSSH)?\s?PRIVATE KEY", RuleKind::Plain),
            ("npm", r"\bnpm_[A-Za-z0-9_]{36}\b", RuleKind::Plain),
            ("slack", r"\b(?:xoxb|xoxp|xapp|xoxa|xoxo|xoxr)-(?:\d-)?(?:[a-zA-Z0-9]{1,40}-)+[a-zA-Z0-9]{1,40}\b", RuleKind::Plain),
            ("slack", r"https://hooks\.slack\.com/services/T[a-zA-Z0-9]{1,40}/B[a-zA-Z0-9]{1,40}/[a-zA-Z0-9]{1,40}", RuleKind::Plain),
            ("basicauth", r"(?:https?|ftp|ftps)://[-a-zA-Z0-9_]{1,256}:[-a-zA-Z0-9_]{1,256}@[-a-zA-Z0-9%._+~#=]{1,256}\.[a-zA-Z0-9()]{1,6}\b", RuleKind::BasicAuth),
            ("openai", r"sk-(?:proj|svcacct|admin)-(?:[A-Za-z0-9_-]{74}|[A-Za-z0-9_-]{58})T3BlbkFJ(?:[A-Za-z0-9_-]{74}|[A-Za-z0-9_-]{58})\b|sk-[a-zA-Z0-9]{20}T3BlbkFJ[a-zA-Z0-9]{20}", RuleKind::Plain),
            ("anthropic", r"\bsk-ant-api0\d-[A-Za-z0-9_-]{90,128}AA\b", RuleKind::Plain),
            ("groq", r"\bgsk_[a-zA-Z0-9]{52}\b", RuleKind::Plain),
            ("huggingface", r"\bhf_[a-zA-Z]{34}\b", RuleKind::Plain),
            ("linear", r"\blin_api_[a-zA-Z0-9_]{32,128}\b", RuleKind::Plain),
            ("notion", r"\bntn_[0-9]{11}[A-Za-z0-9]{35}\b", RuleKind::Plain),
            ("sendgrid", r"\bSG\.[A-Za-z0-9_]{1,128}\.[A-Za-z0-9_]{1,128}(?:[-_]?[A-Za-z0-9_]{1,128})\b", RuleKind::Plain),
            ("shopify", r"\b(?:shppa|shpca|shpat|shpss)_[a-zA-Z0-9]{32,64}\b", RuleKind::Plain),
            ("stripe", r"\b(?:sk_live|sk_test|rk_live|rk_test)_[a-zA-Z0-9]{24,99}\b", RuleKind::Plain),
            ("github", r"\b(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9_]{36}\b", RuleKind::Plain),
            ("github", r"\bgithub_pat_[A-Za-z0-9_]{82}\b", RuleKind::Plain),
            ("gitlab", r"\bglpat-[A-Za-z0-9_-]{20,128}\b", RuleKind::Plain),
            ("grafana", r"\bglc_[A-Za-z0-9+/]{32,400}={0,2}\b", RuleKind::Plain),
            ("grafana", r"\bglsa_[a-zA-Z0-9]{32}_[A-Fa-f0-9]{8}\b", RuleKind::Plain),
            ("1password", r"\bops_ey[A-Za-z0-9+/=]{100,1280}fQ={0,2}\b", RuleKind::Plain),
            ("database-connection-string", r#"mongodb(?:\+srv)?://[^\s"']+"#, RuleKind::Plain),
            ("database-connection-string", r#"(?:jdbc:)?mysql(?:x)?://[^\s"']+"#, RuleKind::Plain),
            ("database-connection-string", r#"postgres(?:ql)?://[^\s"']+"#, RuleKind::Plain),
            ("hashicorp-vault", r"\bhvs\.[A-Za-z0-9_-]{90,120}\b", RuleKind::Plain),
            ("hashicorp-vault", r"\bhvb\.[A-Za-z0-9_-]{138,300}\b", RuleKind::Plain),
            ("hashicorp-vault", r"\bhvr\.[A-Za-z0-9_-]{90,120}\b", RuleKind::Plain),
            ("vercel", r"\b(?:vcp|vci|vca|vcr|vck)_[A-Za-z0-9]{20,60}\b", RuleKind::Plain),
            ("databricks", r"\bdapi[A-Fa-f0-9]{32}(?:-[0-9])?\b", RuleKind::Plain),
            ("docker", r"\bdckr_pat_[a-zA-Z0-9_-]{27}\b", RuleKind::Plain),
            ("figma", r"\bfigd_[A-Za-z0-9_-]{40,200}\b", RuleKind::Plain),
            ("cloudflare", r"\b(?:cfk|cfut|cfat)_[A-Za-z0-9]{40}[0-9a-f]{8}\b", RuleKind::Plain),
            ("tailscale", r"\btskey-[a-z]{2,20}-[0-9A-Za-z_]{8,40}-[0-9A-Za-z_]{16,60}\b", RuleKind::Plain),
        ];
        specs
            .iter()
            .filter_map(|(name, pattern, kind)| {
                Regex::new(pattern).ok().map(|re| Rule {
                    name,
                    re,
                    kind: *kind,
                })
            })
            .collect()
    })
}

/// The result of scanning a single file.
#[derive(Debug, Clone)]
pub struct SecurityResult {
    pub file_path: String,
    pub messages: Vec<String>,
}

/// Scan a piece of text for secrets. Returns the number of issues found.
pub fn count_issues(content: &str) -> usize {
    let mut count = 0usize;
    for rule in rules() {
        match rule.kind {
            RuleKind::Plain => {
                count += rule.re.find_iter(content).count();
            }
            RuleKind::BasicAuth => {
                for m in rule.re.find_iter(content) {
                    if basic_auth_is_suspicious(&m) {
                        count += 1;
                    }
                }
            }
        }
    }
    count
}

/// Scan a file and describe the issues that were found.
pub fn scan_file(file_path: &str, content: &str) -> Option<SecurityResult> {
    let mut messages = Vec::new();
    for rule in rules() {
        match rule.kind {
            RuleKind::Plain => {
                for m in rule.re.find_iter(content) {
                    messages.push(format!(
                        "Found {} pattern in {}: {}",
                        rule.name,
                        file_path,
                        truncate(&m.as_str())
                    ));
                }
            }
            RuleKind::BasicAuth => {
                for m in rule.re.find_iter(content) {
                    if basic_auth_is_suspicious(&m) {
                        messages.push(format!(
                            "Found {} pattern in {}: {}",
                            rule.name,
                            file_path,
                            truncate(&m.as_str())
                        ));
                    }
                }
            }
        }
    }
    if messages.is_empty() {
        None
    } else {
        Some(SecurityResult {
            file_path: file_path.to_string(),
            messages,
        })
    }
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
    fn detects_aws_keys() {
        assert!(count_issues("AKIAIOSFODNN7EXAMPLE") > 0);
    }

    #[test]
    fn detects_private_keys() {
        assert!(count_issues("-----BEGIN RSA PRIVATE KEY-----") > 0);
    }

    #[test]
    fn ignores_placeholder_credentials() {
        assert_eq!(count_issues("https://user:password@example.com/path"), 0);
        assert_eq!(count_issues("https://${USER}:${PASS}@example.com"), 0);
    }

    #[test]
    fn detects_real_credentials() {
        assert!(count_issues("https://admin:Xy9!kQ2mZ@example.org/x") > 0);
    }

    #[test]
    fn plain_code_is_clean() {
        let code = "fn main() { println!(\"hello\"); }";
        assert_eq!(count_issues(code), 0);
    }
}
