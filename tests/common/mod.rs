//! Shared helpers for the integration test suite.

/// Strip the renderer's leading `HH:MM:SS ` timestamp from every line of
/// captured output so assertions can target the `[label] body` part.
/// Lines without the prefix (script `print` output never passes through
/// the renderer) pass through unchanged.
pub fn strip_timestamps(output: &str) -> String {
    let mut stripped = output
        .lines()
        .map(strip_timestamp)
        .collect::<Vec<_>>()
        .join("\n");
    if output.ends_with('\n') {
        stripped.push('\n');
    }
    stripped
}

/// Strip one line's leading `HH:MM:SS ` prefix, if present.
pub fn strip_timestamp(line: &str) -> &str {
    let b = line.as_bytes();
    let is_ts = b.len() >= 9
        && b[0].is_ascii_digit()
        && b[1].is_ascii_digit()
        && b[2] == b':'
        && b[3].is_ascii_digit()
        && b[4].is_ascii_digit()
        && b[5] == b':'
        && b[6].is_ascii_digit()
        && b[7].is_ascii_digit()
        && b[8] == b' ';
    if is_ts { &line[9..] } else { line }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_only_wellformed_prefixes() {
        assert_eq!(strip_timestamp("12:34:56 [mock/s1] hi"), "[mock/s1] hi");
        assert_eq!(strip_timestamp("raw print"), "raw print");
        // Malformed times / no trailing space are left alone.
        assert_eq!(strip_timestamps("1:34:56 x"), "1:34:56 x");
        assert_eq!(strip_timestamps("12:34:56"), "12:34:56");
        assert_eq!(strip_timestamps("12:34:5x a"), "12:34:5x a");
    }
}
