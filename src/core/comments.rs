//! Comment removal (`--remove-comments`).
//!
//! A line-preserving, string-aware comment stripper. Newlines inside removed
//! comments are kept so that line numbers stay stable.

/// The comment syntax used by a language.
#[derive(Clone, Copy)]
struct Syntax {
    /// Line comment introducers, e.g. `//` or `#`.
    line: &'static [&'static str],
    /// Block comment delimiters, if the language has them.
    block: Option<(&'static str, &'static str)>,
    /// Whether single quotes delimit strings.
    single_quote_strings: bool,
    /// Whether the language uses heredocs (shell).
    heredocs: bool,
    /// Whether the language supports YAML block scalars.
    block_scalars: bool,
}

const C_LIKE: Syntax = Syntax {
    line: &["//"],
    block: Some(("/*", "*/")),
    single_quote_strings: true,
    heredocs: false,
    block_scalars: false,
};

const HASH: Syntax = Syntax {
    line: &["#"],
    block: None,
    single_quote_strings: true,
    heredocs: false,
    block_scalars: false,
};

const SHELL: Syntax = Syntax {
    line: &["#"],
    block: None,
    single_quote_strings: true,
    heredocs: true,
    block_scalars: false,
};

const YAML: Syntax = Syntax {
    line: &["#"],
    block: None,
    single_quote_strings: true,
    heredocs: false,
    block_scalars: true,
};

const HTML: Syntax = Syntax {
    line: &[],
    block: Some(("<!--", "-->")),
    single_quote_strings: true,
    heredocs: false,
    block_scalars: false,
};

const SQL: Syntax = Syntax {
    line: &["--"],
    block: Some(("/*", "*/")),
    single_quote_strings: true,
    heredocs: false,
    block_scalars: false,
};

const LESS: Syntax = Syntax {
    line: &["//"],
    block: Some(("/*", "*/")),
    single_quote_strings: true,
    heredocs: false,
    block_scalars: false,
};

const SASS: Syntax = Syntax {
    line: &["//"],
    block: Some(("/*", "*/")),
    single_quote_strings: true,
    heredocs: false,
    block_scalars: false,
};

/// Resolve the comment syntax for a file extension.
fn syntax_for_ext(ext: &str) -> Option<Syntax> {
    let syntax = match ext {
        "c" | "h" | "dart" | "kt" | "rs" | "sol" => C_LIKE,
        "hpp" | "cpp" | "cc" | "cxx" => C_LIKE,
        "cs" => C_LIKE,
        "go" => C_LIKE,
        "html" => HTML,
        "java" => C_LIKE,
        "js" | "jsx" | "mjs" | "cjs" | "mjsx" | "ts" | "tsx" | "mts" | "cts" | "mtsx" => C_LIKE,
        "less" => LESS,
        "php" => Syntax {
            line: &["//", "#"],
            block: Some(("/*", "*/")),
            single_quote_strings: true,
            heredocs: false,
            block_scalars: false,
        },
        "py" => HASH,
        "rb" => HASH,
        "sh" => SHELL,
        "sass" | "scss" => SASS,
        "sql" => SQL,
        "swift" => C_LIKE,
        "xml" => HTML,
        "yaml" | "yml" => YAML,
        "css" => Syntax {
            line: &[],
            block: Some(("/*", "*/")),
            single_quote_strings: true,
            heredocs: false,
            block_scalars: false,
        },
        _ => return None,
    };
    Some(syntax)
}

/// Does this extension support comment removal at all?
pub fn supports_comments(ext: &str) -> bool {
    syntax_for_ext(ext).is_some()
}

/// Remove comments from the given content.
pub fn remove_comments(content: &str, ext: &str) -> Option<String> {
    let syntax = syntax_for_ext(ext)?;
    let content = if ext == "py" {
        strip_python_docstrings(content)
    } else {
        content.to_string()
    };
    Some(rtrim_lines(&strip(&content, &syntax)))
}

/// Remove Python docstrings (triple quoted strings in docstring position).
fn strip_python_docstrings(content: &str) -> String {
    let lines: Vec<&str> = content.split('\n').collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut i = 0usize;
    // True when the next statement could be a docstring.
    let mut docstring_position = true;
    let mut in_string: Option<&str> = None;

    while i < lines.len() {
        let line = lines[i];
        if let Some(delim) = in_string {
            if let Some(pos) = line.find(delim) {
                let rest = &line[pos + delim.len()..];
                out.push(rest.to_string());
                in_string = None;
                docstring_position = false;
            } else {
                out.push(String::new());
            }
            i += 1;
            continue;
        }

        let trimmed = line.trim_start();
        if docstring_position
            && (trimmed.starts_with("\"\"\"") || trimmed.starts_with("'''"))
        {
            let delim = if trimmed.starts_with("\"\"\"") {
                "\"\"\""
            } else {
                "'''"
            };
            let indent = &line[..line.len() - trimmed.len()];
            let after = &trimmed[delim.len()..];
            if let Some(pos) = after.find(delim) {
                let rest = &after[pos + delim.len()..];
                out.push(format!("{indent}{rest}"));
            } else {
                in_string = Some(delim);
                out.push(String::new());
            }
            docstring_position = false;
            i += 1;
            continue;
        }

        out.push(line.to_string());
        docstring_position = trimmed.is_empty()
            || trimmed.starts_with('#')
            || trimmed.starts_with('@')
            || trimmed.ends_with(':');
        i += 1;
    }

    out.join("\n")
}

/// Trim trailing whitespace from every line.
fn rtrim_lines(content: &str) -> String {
    let mut out = String::with_capacity(content.len());
    for (i, line) in content.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(line.trim_end());
    }
    out
}

/// Strip comments according to the given syntax.
fn strip(content: &str, syntax: &Syntax) -> String {
    let bytes = content.as_bytes();
    let mut out = String::with_capacity(content.len());
    let mut i = 0usize;
    // Heredoc state: the terminator we are waiting for, if any.
    let mut heredoc: Option<String> = None;
    // YAML block scalar indent, if any.
    let mut block_scalar_indent: Option<usize> = None;
    let mut at_line_start = true;
    let mut line_indent = 0usize;

    while i < bytes.len() {
        if let Some(terminator) = &heredoc {
            // Copy the line verbatim until the terminator is found.
            let line_start = i;
            let mut line_end = i;
            while line_end < bytes.len() && bytes[line_end] != b'\n' {
                line_end += 1;
            }
            let line = &content[line_start..line_end];
            if line.trim() == terminator.as_str() {
                heredoc = None;
            }
            out.push_str(line);
            if line_end < bytes.len() {
                out.push('\n');
                i = line_end + 1;
                at_line_start = true;
            } else {
                i = line_end;
            }
            continue;
        }

        let c = bytes[i];

        // Newline handling.
        if c == b'\n' {
            out.push('\n');
            i += 1;
            at_line_start = true;
            line_indent = 0;
            continue;
        }

        if syntax.block_scalars {
            if let Some(indent) = block_scalar_indent {
                // Inside a block scalar: copy verbatim until a non blank line
                // with an indent <= the block indent.
                if c == b' ' {
                    out.push(' ');
                    i += 1;
                    line_indent += 1;
                    continue;
                }
                if c == b'\t' {
                    out.push('\t');
                    i += 1;
                    line_indent += 1;
                    continue;
                }
                if c == b'\n' {
                    out.push('\n');
                    i += 1;
                    at_line_start = true;
                    line_indent = 0;
                    continue;
                }
                if line_indent <= indent {
                    block_scalar_indent = None;
                } else {
                    out.push(c as char);
                    i += 1;
                    continue;
                }
            }
        }

        // Block comments.
        if let Some((open, close)) = syntax.block {
            if bytes[i..].starts_with(open.as_bytes()) {
                let mut j = i + open.len();
                while j < bytes.len() {
                    if bytes[j..].starts_with(close.as_bytes()) {
                        j += close.len();
                        break;
                    }
                    if bytes[j] == b'\n' {
                        out.push('\n');
                    }
                    j += 1;
                }
                i = j;
                continue;
            }
        }

        // Heredoc detection (shell).
        if syntax.heredocs && (at_line_start || c == b'<' || c == b'=') {
            if let Some(delim) = detect_heredoc(content, i) {
                if heredoc.is_none() {
                    heredoc = Some(delim);
                    // Copy the remainder of the line verbatim.
                    let line_start = i;
                    let mut line_end = i;
                    while line_end < bytes.len() && bytes[line_end] != b'\n' {
                        line_end += 1;
                    }
                    out.push_str(&content[line_start..line_end]);
                    i = line_end;
                    continue;
                }
            }
        }

        // Strings.
        if c == b'"' || (c == b'\'' && syntax.single_quote_strings) {
            let quote = c;
            out.push(c as char);
            i += 1;
            while i < bytes.len() {
                let d = bytes[i];
                if d == b'\\' && quote == b'"' && i + 1 < bytes.len() {
                    out.push('\\');
                    out.push(bytes[i + 1] as char);
                    i += 2;
                    continue;
                }
                out.push(d as char);
                i += 1;
                if d == quote {
                    break;
                }
                if d == b'\n' {
                    at_line_start = true;
                    break;
                }
            }
            continue;
        }

        // Line comments.
        let mut matched = false;
        for prefix in syntax.line {
            if bytes[i..].starts_with(prefix.as_bytes()) {
                // A `#` only starts a comment at the beginning of a line or
                // after whitespace.
                if *prefix == "#" {
                    let prev = if i > 0 { bytes[i - 1] } else { b' ' };
                    if !(at_line_start || prev == b' ' || prev == b'\t') {
                        continue;
                    }
                }
                // Skip to the end of the line, keeping the newline.
                let mut j = i;
                while j < bytes.len() && bytes[j] != b'\n' {
                    j += 1;
                }
                i = j;
                matched = true;
                break;
            }
        }
        if matched {
            continue;
        }

        // YAML block scalar detection.
        if syntax.block_scalars && (c == b'|' || c == b'>') {
            let prev = if i > 0 { bytes[i - 1] } else { b' ' };
            if prev == b':' || prev == b' ' || prev == b'-' {
                let rest = &content[i..];
                let trimmed = rest.trim_start_matches(|c: char| matches!(c, '|' | '>' | '+' | '-' | '0'..='9'));
                if trimmed.starts_with('\n') || trimmed.is_empty() {
                    block_scalar_indent = Some(line_indent + 1);
                }
            }
        }

        out.push(c as char);
        if c == b' ' || c == b'\t' {
            line_indent += 1;
        }
        at_line_start = false;
        i += 1;
    }

    out
}

/// Detect a shell heredoc introducer at `pos`, returning the terminator.
fn detect_heredoc(content: &str, pos: usize) -> Option<String> {
    let bytes = content.as_bytes();
    if pos >= bytes.len() || bytes[pos] != b'<' {
        return None;
    }
    let mut i = pos + 1;
    if i < bytes.len() && bytes[i] == b'<' {
        i += 1;
    } else {
        return None;
    }
    // Optional dash.
    if i < bytes.len() && bytes[i] == b'-' {
        i += 1;
    }
    // Optional quote.
    let quote = if i < bytes.len() && (bytes[i] == b'\'' || bytes[i] == b'"') {
        let q = bytes[i];
        i += 1;
        Some(q)
    } else {
        None
    };
    let start = i;
    while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
        i += 1;
    }
    if i == start {
        return None;
    }
    let delim = content[start..i].to_string();
    if let Some(q) = quote {
        if i < bytes.len() && bytes[i] == q {
            i += 1;
        }
    }
    Some(delim)
}

/// Remove empty lines from the content.
pub fn remove_empty_lines(content: &str) -> String {
    content
        .split('\n')
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_line_comments() {
        let out = remove_comments("// hello\nlet x = 1; // trailing\n", "rs").unwrap();
        assert_eq!(out, "\nlet x = 1;\n");
    }

    #[test]
    fn keeps_strings_intact() {
        let out = remove_comments("let s = \"// not a comment\";\n", "rs").unwrap();
        assert_eq!(out, "let s = \"// not a comment\";\n");
    }

    #[test]
    fn removes_block_comments_but_keeps_newlines() {
        let out = remove_comments("a /* one\ntwo */ b\n", "c").unwrap();
        assert_eq!(out, "a  \n b\n");
    }

    #[test]
    fn python_hash_comments() {
        let out = remove_comments("# comment\nx = 1  # trailing\n", "py").unwrap();
        assert_eq!(out, "\nx = 1  \n");
    }

    #[test]
    fn unsupported_extension_is_none() {
        assert!(remove_comments("whatever", "unknown-ext").is_none());
    }
}
