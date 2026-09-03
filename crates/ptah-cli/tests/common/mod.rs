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

/// Count live processes whose `/proc` cmdline contains `needle`. The
/// suite tags test sleeps with unique argv values, so a needle like
/// `"9871"` matches exactly the processes a test cares about.
pub fn count_processes(needle: &str) -> usize {
    let mut n = 0;
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return 0;
    };
    for entry in entries.flatten() {
        let Ok(raw) = std::fs::read(entry.path().join("cmdline")) else {
            continue;
        };
        if raw
            .split(|b| *b == 0)
            .any(|arg| std::str::from_utf8(arg).is_ok_and(|s| s.contains(needle)))
        {
            n += 1;
        }
    }
    n
}

/// SIGKILL every live process whose `/proc` cmdline contains `needle`
/// (same match rule as [`count_processes`]). For sweeping orphans a
/// previously failed test run may have left under a stable tag, so the
/// next run's no-orphan assertion is about *its own* processes — never
/// for anything the current run still owns.
pub fn kill_processes(needle: &str) {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(raw) = std::fs::read(entry.path().join("cmdline")) else {
            continue;
        };
        let hit = raw
            .split(|b| *b == 0)
            .any(|arg| std::str::from_utf8(arg).is_ok_and(|s| s.contains(needle)));
        if hit
            && let Some(name) = entry.file_name().to_str()
            && let Ok(pid) = name.parse::<i32>()
        {
            // SAFETY: a plain SIGKILL to a pid we just observed.
            unsafe { libc::kill(pid, libc::SIGKILL) };
        }
    }
}

/// Poll `count_processes` until it reaches `want` (up to 5s), else
/// panic naming `what` — the suite's "no orphans" witness.
pub fn wait_for_processes(needle: &str, want: usize, what: &str) {
    for _ in 0..250 {
        if count_processes(needle) == want {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    panic!(
        "expected {what} (count {want}) for processes tagged {needle:?}, got {}",
        count_processes(needle)
    );
}

/// Sweep stale processes a previously killed run left under a stable
/// tag, then wait for the sweep to land — the pre-run half of the
/// suite's no-orphan witnessing. Call this at the start of any test
/// asserting a stable tag (the e2e exec tags, the cli signal tags), so
/// the witness is about *its own* processes, never a leftover: a
/// SIGKILLed run skips drop-time kills and orphans its tagged sleeps.
pub fn clear_stale_tag(tag: &str) {
    kill_processes(tag);
    wait_for_processes(tag, 0, "stale tag cleared before the run");
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
