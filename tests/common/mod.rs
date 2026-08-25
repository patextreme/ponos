//! Shared helpers for the integration test suite.

/// Strip the renderer's leading `yyyy-mm-dd HH:MM:SS ` timestamp from
/// every line of captured output so assertions can target the
/// `[label] body` part. Lines without the prefix (script `print` output
/// never passes through the renderer) pass through unchanged.
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

/// Strip one line's leading `yyyy-mm-dd HH:MM:SS ` prefix, if present.
pub fn strip_timestamp(line: &str) -> &str {
    let b = line.as_bytes();
    let is_ts = b.len() >= 20
        && b[0..4].iter().all(u8::is_ascii_digit)
        && b[4] == b'-'
        && b[5..7].iter().all(u8::is_ascii_digit)
        && b[7] == b'-'
        && b[8..10].iter().all(u8::is_ascii_digit)
        && b[10] == b' '
        && b[11..13].iter().all(u8::is_ascii_digit)
        && b[13] == b':'
        && b[14..16].iter().all(u8::is_ascii_digit)
        && b[16] == b':'
        && b[17..19].iter().all(u8::is_ascii_digit)
        && b[19] == b' ';
    if is_ts { &line[20..] } else { line }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_only_wellformed_prefixes() {
        assert_eq!(
            strip_timestamp("2026-08-25 12:34:56 [mock/s1] hi"),
            "[mock/s1] hi"
        );
        assert_eq!(strip_timestamp("raw print"), "raw print");
        // Malformed dates/times / no trailing space are left alone.
        assert_eq!(
            strip_timestamps("26-08-25 12:34:56 x"),
            "26-08-25 12:34:56 x"
        );
        assert_eq!(
            strip_timestamps("2026-08-25 12:34:56"),
            "2026-08-25 12:34:56"
        );
        assert_eq!(
            strip_timestamps("2026-08-25 12:34:5x a"),
            "2026-08-25 12:34:5x a"
        );
    }
}
