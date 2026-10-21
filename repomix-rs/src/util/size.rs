//! Human readable size parsing (`500kb`, `2mb`, `2.5mb`).

use anyhow::{bail, Result};

/// Parse a human readable size into bytes.
///
/// `kb` is 1024 bytes and `mb` is 1024 * 1024 bytes, matching Repomix.
pub fn parse_human_size(input: &str) -> Result<u64> {
    let trimmed = input.trim();
    let lower = trimmed.to_ascii_lowercase();

    let (number, unit) = if let Some(rest) = lower.strip_suffix("kb") {
        (rest, "kb")
    } else if let Some(rest) = lower.strip_suffix("mb") {
        (rest, "mb")
    } else {
        bail!(
            "Invalid size: '{input}'. Expected format like '500kb', '2mb', or '2.5mb' (case-insensitive)."
        );
    };

    let number = number.trim();
    if number.is_empty() || !number.chars().all(|c| c.is_ascii_digit() || c == '.') {
        bail!("Invalid size: '{input}'. Expected format like '500kb', '2mb', or '2.5mb' (case-insensitive).");
    }

    let amount: f64 = match number.parse() {
        Ok(v) => v,
        Err(_) => bail!("Invalid size amount: '{number}'. Must be a positive number."),
    };
    if !amount.is_finite() || amount <= 0.0 {
        bail!("Invalid size amount: '{number}'. Must be a positive number.");
    }

    let multiplier: f64 = if unit == "kb" { 1024.0 } else { 1024.0 * 1024.0 };
    let bytes = (amount * multiplier).floor() as u64;
    if bytes < 1 {
        bail!("Invalid size: '{input}'. Resulting size must be at least 1 byte.");
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sizes() {
        assert_eq!(parse_human_size("500kb").unwrap(), 512000);
        assert_eq!(parse_human_size("2mb").unwrap(), 2097152);
        assert_eq!(parse_human_size("2.5mb").unwrap(), 2621440);
        assert_eq!(parse_human_size(" 1 MB ").unwrap(), 1048576);
        assert!(parse_human_size("10").is_err());
        assert!(parse_human_size("0kb").is_err());
        assert!(parse_human_size("abc").is_err());
    }
}
