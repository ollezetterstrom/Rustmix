//! Base64 truncation (`--truncate-base64`).

const MIN_BASE64_LENGTH_DATA_URI: usize = 40;
const MIN_BASE64_LENGTH_STANDALONE: usize = 256;
const TRUNCATION_LENGTH: usize = 32;
const MIN_CHAR_DIVERSITY: usize = 10;
const MIN_CHAR_TYPE_COUNT: usize = 3;

/// Truncate long base64 payloads, long SVG path data, and minified lines > 500 chars.
pub fn truncate_base64_content(content: &str) -> String {
    let mut processed = content.to_string();

    if processed.contains(";base64,") {
        processed = truncate_data_uris(&processed);
    }

    if has_long_base64_run(&processed) {
        processed = truncate_standalone(&processed);
    }

    let lower = processed.to_ascii_lowercase();
    if lower.contains("<path") {
        processed = truncate_svg_paths(&processed);
    }

    processed = truncate_long_lines(&processed);

    processed
}

/// Truncate long inline SVG `<path d="...">` data.
pub fn truncate_svg_paths(content: &str) -> String {
    let lower = content.to_ascii_lowercase();
    if !lower.contains("<path") {
        return content.to_string();
    }

    let mut out = String::with_capacity(content.len());
    let mut cursor = 0usize;

    while let Some(path_start) = lower[cursor..].find("<path") {
        let abs_path_start = cursor + path_start;
        if let Some(tag_end_rel) = content[abs_path_start..].find('>') {
            let abs_tag_end = abs_path_start + tag_end_rel + 1;
            let tag_content = &content[abs_path_start..abs_tag_end];

            let mut replaced_tag = None;
            for quote in ['"', '\''] {
                let needle = format!("d={quote}");
                if let Some(d_start) = tag_content.find(&needle) {
                    let val_start = d_start + needle.len();
                    if let Some(val_len) = tag_content[val_start..].find(quote) {
                        if val_len > 60 {
                            let val = &tag_content[val_start..val_start + val_len];
                            let preview: String = val.chars().take(TRUNCATION_LENGTH).collect();
                            let new_tag = format!(
                                "{}{preview}...{}",
                                &tag_content[..val_start],
                                &tag_content[val_start + val_len..]
                            );
                            replaced_tag = Some(new_tag);
                            break;
                        }
                    }
                }
            }

            out.push_str(&content[cursor..abs_path_start]);
            if let Some(new_tag) = replaced_tag {
                out.push_str(&new_tag);
            } else {
                out.push_str(tag_content);
            }
            cursor = abs_tag_end;
        } else {
            break;
        }
    }
    out.push_str(&content[cursor..]);
    out
}

/// Truncate single lines > 500 characters (minified blobs).
pub fn truncate_long_lines(content: &str) -> String {
    const MAX_LINE_LEN: usize = 500;
    if !content.lines().any(|l| l.len() > MAX_LINE_LEN) {
        return content.to_string();
    }
    let mut out = String::with_capacity(content.len());
    let mut first = true;
    for line in content.split('\n') {
        if !first {
            out.push('\n');
        }
        first = false;
        if line.len() > MAX_LINE_LEN {
            let mut end = MAX_LINE_LEN;
            while end > 0 && !line.is_char_boundary(end) {
                end -= 1;
            }
            out.push_str(&line[..end]);
            out.push_str("... [truncated]");
        } else {
            out.push_str(line);
        }
    }
    out
}

/// Replace `data:<mime>[;params];base64,<payload>` with a shortened preview.
fn truncate_data_uris(content: &str) -> String {
    let mut out = String::with_capacity(content.len());
    let bytes = content.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i..].starts_with(b"data:") {
            if let Some(end) = find_data_uri_end(bytes, i) {
                let segment = &content[i..end];
                if let Some(replacement) = shorten_data_uri(segment) {
                    out.push_str(&replacement);
                    i = end;
                    continue;
                }
            }
        }
        // Copy one UTF-8 character.
        let ch = content[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// Find the end of a data URI starting at `start`.
fn find_data_uri_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut i = start + 5; // skip "data:"
    while i < bytes.len() {
        let c = bytes[i];
        if c == b';' || c == b',' {
            break;
        }
        if !c.is_ascii_alphanumeric() && c != b'/' && c != b'-' && c != b'+' {
            return None;
        }
        i += 1;
    }
    if i >= bytes.len() || bytes[i] != b';' {
        return None;
    }
    // Parameters.
    while i < bytes.len() && bytes[i] == b';' {
        i += 1;
        let param_start = i;
        while i < bytes.len() && bytes[i] != b';' && bytes[i] != b',' {
            i += 1;
        }
        if i >= bytes.len() {
            return None;
        }
        let param = &bytes[param_start..i];
        if !param.iter().all(|b| b.is_ascii_alphanumeric() || *b == b'-' || *b == b'=') {
            return None;
        }
    }
    if i >= bytes.len() || bytes[i] != b',' {
        return None;
    }
    i += 1;
    let data_start = i;
    while i < bytes.len() && is_base64_char(bytes[i]) {
        i += 1;
    }
    // Trailing padding.
    while i < bytes.len() && bytes[i] == b'=' {
        i += 1;
    }
    if i - data_start < MIN_BASE64_LENGTH_DATA_URI {
        return None;
    }
    Some(i)
}

fn is_base64_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'+' || b == b'/' || b == b'='
}

/// Shorten a single data URI, returning `None` when it does not qualify.
fn shorten_data_uri(segment: &str) -> Option<String> {
    let marker = ";base64,";
    let idx = segment.find(marker)?;
    let head = &segment[..idx + marker.len()];
    let payload = &segment[idx + marker.len()..];
    if payload.len() < MIN_BASE64_LENGTH_DATA_URI {
        return None;
    }
    if !payload
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/' || b == b'=')
    {
        return None;
    }
    let preview: String = payload.chars().take(TRUNCATION_LENGTH).collect();
    Some(format!("{head}{preview}..."))
}

/// Is there a run of at least 256 base64 characters?
fn has_long_base64_run(content: &str) -> bool {
    if content.len() < MIN_BASE64_LENGTH_STANDALONE {
        return false;
    }
    let mut run = 0usize;
    for &c in content.as_bytes() {
        if c.is_ascii_alphanumeric() || c == b'+' || c == b'/' {
            run += 1;
            if run >= MIN_BASE64_LENGTH_STANDALONE {
                return true;
            }
        } else {
            run = 0;
        }
    }
    false
}

/// Replace standalone base64 runs with a shortened preview.
fn truncate_standalone(content: &str) -> String {
    let bytes = content.as_bytes();
    let mut out = String::with_capacity(content.len());
    let mut i = 0usize;
    while i < bytes.len() {
        if is_base64_char(bytes[i]) && bytes[i] != b'=' {
            let start = i;
            while i < bytes.len() && is_base64_char(bytes[i]) && bytes[i] != b'=' {
                i += 1;
            }
            let mut end = i;
            let mut padding = 0usize;
            while end < bytes.len() && bytes[end] == b'=' && padding < 2 {
                end += 1;
                padding += 1;
            }
            let candidate = &content[start..end];
            if candidate.len() >= MIN_BASE64_LENGTH_STANDALONE && is_likely_base64(candidate) {
                let preview: String = candidate.chars().take(TRUNCATION_LENGTH).collect();
                out.push_str(&preview);
                out.push_str("...");
            } else {
                out.push_str(candidate);
            }
            i = end;
        } else {
            let ch = content[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

/// Heuristic check that a string really looks like base64 data.
fn is_likely_base64(s: &str) -> bool {
    let mut has_numbers = false;
    let mut has_upper = false;
    let mut has_lower = false;
    let mut has_special = false;
    let mut distinct = std::collections::HashSet::new();

    for c in s.chars() {
        match c {
            '=' => continue,
            '0'..='9' => {
                has_numbers = true;
                distinct.insert(c);
            }
            'A'..='Z' => {
                has_upper = true;
                distinct.insert(c);
            }
            'a'..='z' => {
                has_lower = true;
                distinct.insert(c);
            }
            '+' | '/' => {
                has_special = true;
                distinct.insert(c);
            }
            _ => return false,
        }
    }
    if distinct.len() < MIN_CHAR_DIVERSITY {
        return false;
    }
    if !has_numbers {
        return false;
    }
    let types = [has_numbers, has_upper, has_lower, has_special]
        .iter()
        .filter(|v| **v)
        .count();
    types >= MIN_CHAR_TYPE_COUNT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncates_data_uris() {
        let payload = "A".repeat(60);
        let content = format!("x data:image/png;base64,{payload} y");
        let out = truncate_base64_content(&content);
        assert!(out.contains(&format!("data:image/png;base64,{}...", "A".repeat(32))));
    }

    #[test]
    fn leaves_short_payloads_alone() {
        let content = "data:image/png;base64,QUJD";
        assert_eq!(truncate_base64_content(content), content);
    }

    #[test]
    fn truncates_standalone_runs() {
        let payload: String = "aB3cD4eF5gH6".repeat(30);
        let content = format!("prefix {payload} suffix");
        let out = truncate_base64_content(&content);
        assert!(out.ends_with("... suffix"));
        assert!(out.len() < content.len());
    }

    #[test]
    fn truncates_long_svg_path_data() {
        let long_d = "M10 20 L30 40 ".repeat(10);
        let content = format!("<svg><path fill=\"red\" d=\"{long_d}\" /></svg>");
        let out = truncate_base64_content(&content);
        assert!(out.contains("...\""));
        assert!(out.len() < content.len());
    }

    #[test]
    fn truncates_long_lines() {
        let long_line = "console.log('minified');".repeat(30); // > 500 chars
        assert!(long_line.len() > 500);
        let content = format!("line 1\n{long_line}\nline 3");
        let out = truncate_base64_content(&content);
        assert!(out.contains("... [truncated]"));
        assert!(out.starts_with("line 1\n"));
        assert!(out.ends_with("\nline 3"));
    }
}
