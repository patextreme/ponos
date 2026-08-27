//! Dependency-direction guard for `ptah-core`.
//!
//! Core's I/O-freedom and adapter-freedom are the invariants the
//! hexagonal sequence left to convention: ① audited them by hand, ②
//! made the crate arrows compiler-enforced, and this test pins the
//! inside of the boundary in code — a commit that adds forbidden I/O
//! or adapter coupling to core fails `cargo test -p ptah-core`
//! instead of relying on review vigilance.
//!
//! The scanner is deliberately lexical (comment-stripped substring
//! scan over `CARGO_MANIFEST_DIR/src/**.rs` with `use`-tree groups
//! flattened), not semantic: it guards human-written drift; the
//! compiler guards the big crate arrows. Zero external crates, no
//! network — runs everywhere `cargo test` runs, including the nix
//! sandbox.
//!
//! # Forbidden everywhere in `src/`
//!
//! - `std::fs`, `std::process`, `std::net`, `std::io`, `std::thread` —
//!   core does no I/O of its own; every real side effect belongs to an
//!   adapter.
//! - any `tokio::` path outside the allowlist — core is runtime-free.
//! - `mlua` outside `src/task.rs`.
//! - the adapter crates (`ptah_acp`, `ptah_render`, `ptah_check`,
//!   `ptah_luau`, `ptah_config`, `ptah_result`, `ptah_cli`) in any
//!   form — the dependency arrow points inward only.
//! - `agent_client_protocol` outside `turn`/`session`/`ports`/`events`
//!   and, inside them, anything but `agent_client_protocol::schema::`
//!   (schema data types, never the connection/client machinery).
//!
//! # Allowlist (settled exceptions, each with its ①-era reason)
//!
//! - `std::env` reads — `HOME` resolution in `turn`, `${VAR}`
//!   interpolation in `config`.
//! - `std::sync`, `std::path`, `std::time`, `std::collections`, … —
//!   plain data and shared-state types.
//! - `tokio::sync` — the async channel/lock primitives (`mpsc`,
//!   `oneshot`, `watch`, `Mutex`, `Notify`) the session/turn plumbing
//!   is built from.
//! - `tokio::task::spawn_local` — `src/task.rs` only: the spawn
//!   bookkeeping drives mlua coroutines on the current `LocalSet`.
//! - `mlua::{Function, Lua, MultiValue}` and `mlua::Error`/`mlua::Result`
//!   — `src/task.rs` only: task results *are* Luau values; value-level
//!   types, no interpreter surface.
//! - `agent_client_protocol::schema::v1::*` in `turn`, `session`,
//!   `ports`, `events` — the fold's input is the ACP update stream;
//!   schema data types only.
//!
//! Widening any exception is a one-line edit here, visible in review.

use std::fs;
use std::path::{Path, PathBuf};

/// std modules core may never touch: real I/O or thread spawning.
const FORBIDDEN_STD: &[&str] = &[
    "std::fs",
    "std::process",
    "std::net",
    "std::io",
    "std::thread",
];

/// Adapter crates — core never imports them; arrows point inward.
const ADAPTER_CRATES: &[&str] = &[
    "ptah_acp",
    "ptah_render",
    "ptah_check",
    "ptah_luau",
    "ptah_config",
    "ptah_result",
    "ptah_cli",
];

/// Modules allowed to fold `agent_client_protocol` schema data types.
const ACP_SCHEMA_MODULES: &[&str] = &["turn", "session", "ports", "events"];

/// The one file allowed to touch mlua and `spawn_local` (data level).
const TASK_FILE: &str = "src/task.rs";

/// Value-level mlua items `task.rs` may name: ①'s verified import list
/// (`Function`, `Lua`, `MultiValue`) plus the `Error`/`Result` aliases
/// those values flow through.
const TASK_MLUA_ITEMS: &[&str] = &["Function", "Lua", "MultiValue", "Error", "Result"];

struct Violation {
    file: String,
    line: usize,
    msg: String,
}

impl Violation {
    fn render(&self) -> String {
        format!("  {}:{}: {}", self.file, self.line, self.msg)
    }
}

/// One `use` statement: byte range in the stripped text plus every
/// full import path it expands to (`use a::{b, c::d};` → `a::b`,
/// `a::c::d`), so grouped imports can't smuggle a path past the
/// scanner.
struct UseStmt {
    start: usize,
    end: usize,
    paths: Vec<String>,
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Blank `//`, `///`, and `//!` comments with spaces so byte offsets —
/// and therefore line numbers — survive stripping. A `//` inside a
/// string literal also blanks the rest of its line: that can only hide
/// text from the scan (a false negative on that line), never invent a
/// hit. Prose mentions of mlua/ACP in doc comments are exactly what
/// this must neutralize.
fn blank_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    for line in src.split_inclusive('\n') {
        match line.find("//") {
            Some(at) => {
                out.push_str(&line[..at]);
                out.push_str(&" ".repeat(line[at..].len()));
            }
            None => out.push_str(line),
        }
    }
    out
}

/// Find every `use …;` statement (multi-line group imports included)
/// and flatten its tree to full paths. `self` collapses to the prefix;
/// ` as alias` suffixes drop; the alias only ever renames, it cannot
/// change which crate is imported.
fn extract_use_stmts(text: &str) -> Vec<UseStmt> {
    let b = text.as_bytes();
    let mut stmts = Vec::new();
    let mut search_from = 0;
    while let Some(rel) = text[search_from..].find("use") {
        let at = search_from + rel;
        let after = at + "use".len();
        search_from = after;
        let boundary_before = at == 0 || !is_ident_byte(b[at - 1]);
        let boundary_after = after >= b.len() || !is_ident_byte(b[after]);
        let followed_by_space = matches!(
            b.get(after),
            Some(&b' ') | Some(&b'\t') | Some(&b'\n') | Some(&b'\r')
        );
        if !boundary_before || !boundary_after || !followed_by_space {
            continue; // `user`, `house`, … — not the keyword
        }
        let Some(semi_rel) = text[after..].find(';') else {
            continue; // a `use` token in a string, not a statement
        };
        let end = after + semi_rel + 1;
        let paths = expand_paths(&text[after..end - 1]);
        stmts.push(UseStmt {
            start: at,
            end,
            paths,
        });
        search_from = end;
    }
    stmts
}

/// Flatten a use-tree body (the text between `use` and `;`).
fn expand_paths(tail: &str) -> Vec<String> {
    let mut out = Vec::new();
    walk_use_tree("", tail, &mut out);
    out
}

fn walk_use_tree(prefix: &str, rest: &str, out: &mut Vec<String>) {
    let rest = rest.trim();
    if let Some((head, group, _after)) = split_group(rest) {
        let prefix = format!("{prefix}{head}");
        for item in split_top_commas(group) {
            let item = item.trim();
            if item.is_empty() {
                continue;
            }
            if item == "self" {
                out.push(prefix.trim_end_matches(':').to_string());
            } else {
                walk_use_tree(&prefix, item, out);
            }
        }
    } else {
        // Plain leaf; drop an ` as alias` suffix if present.
        let path = match rest.find(" as ") {
            Some(i) => &rest[..i],
            None => rest,
        };
        out.push(format!("{prefix}{path}").trim().to_string());
    }
}

/// Split `head{group}after` at the first brace group (depth-matched).
fn split_group(rest: &str) -> Option<(&str, &str, &str)> {
    let open = rest.find('{')?;
    let mut depth = 0usize;
    for (i, c) in rest[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some((
                        &rest[..open],
                        &rest[open + 1..open + i],
                        &rest[open + i + 1..],
                    ));
                }
            }
            _ => {}
        }
    }
    None // unbalanced — treat as a plain leaf
}

/// Split on commas that sit at brace depth 0.
fn split_top_commas(group: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    for (i, c) in group.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            ',' if depth == 0 => {
                out.push(&group[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&group[start..]);
    out
}

fn find_all(text: &str, needle: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(rel) = text[from..].find(needle) {
        let at = from + rel;
        out.push(at);
        from = at + needle.len();
    }
    out
}

/// The path-looking text starting at `at` (identifiers, `::`, `*`) —
/// e.g. `tokio::sync::Mutex<()>` yields `tokio::sync::Mutex`.
fn path_from(text: &str, at: usize) -> String {
    let b = text.as_bytes();
    let mut end = at;
    while end < b.len() {
        let c = b[end];
        if is_ident_byte(c) || c == b':' || c == b'*' {
            end += 1;
        } else {
            break;
        }
    }
    text[at..end].to_string()
}

/// Allowed tokio surface: `tokio::sync` anywhere; exactly
/// `tokio::task::spawn_local` and only in task.rs. Anything else in
/// the tokio tree is a violation, listed members included.
fn tokio_violation(path: &str, is_task_file: bool) -> Option<String> {
    let rest = path.strip_prefix("tokio::")?;
    if rest.split("::").next() == Some("sync") {
        return None;
    }
    if is_task_file && path == "tokio::task::spawn_local" {
        return None;
    }
    Some(format!(
        "tokio path `{path}` — core allows only `tokio::sync`, plus \
         `tokio::task::spawn_local` in task.rs"
    ))
}

/// ACP is folded as schema data in exactly four modules, and even
/// there only under `agent_client_protocol::schema::` — never the
/// connection/client types.
fn acp_violation(path: &str, module: &str) -> Option<String> {
    let allowed_module = ACP_SCHEMA_MODULES.contains(&module);
    if allowed_module && path.starts_with("agent_client_protocol::schema::") {
        return None;
    }
    if allowed_module {
        Some(format!(
            "`{path}` — only `agent_client_protocol::schema::` data types \
             are allowed, never the connection/client machinery"
        ))
    } else {
        Some(format!(
            "`{path}` — agent_client_protocol is folded only in {}",
            ACP_SCHEMA_MODULES.join("/")
        ))
    }
}

fn scan_file(rel: &str, src: &str, out: &mut Vec<Violation>) {
    let stripped = blank_comments(src);
    let stmts = extract_use_stmts(&stripped);
    let line = |at: usize| 1 + stripped[..at].matches('\n').count();
    let mut push = |at: usize, msg: String| {
        out.push(Violation {
            file: rel.to_string(),
            line: line(at),
            msg,
        });
    };

    let is_task_file = rel == TASK_FILE;
    let module = rel
        .strip_prefix("src/")
        .and_then(|r| r.split('/').next())
        .map(|m| m.trim_end_matches(".rs"))
        .unwrap_or_default();
    let in_use = |at: usize| stmts.iter().any(|s| s.start <= at && at < s.end);

    // Flatten all imports once; grouped trees can't hide members.
    let imports: Vec<(usize, &str)> = stmts
        .iter()
        .flat_map(|s| s.paths.iter().map(move |p| (s.start, p.as_str())))
        .collect();

    // Adapter crates, any form (import or path).
    for name in ADAPTER_CRATES {
        for (at, path) in imports.iter().filter(|(_, p)| p.contains(name)) {
            push(
                *at,
                format!(
                    "adapter crate `{name}` (in `{path}`) — the dependency arrow points inward only"
                ),
            );
        }
        for at in find_all(&stripped, name) {
            if !in_use(at) {
                push(
                    at,
                    format!("adapter crate `{name}` — the dependency arrow points inward only"),
                );
            }
        }
    }

    // Forbidden std paths (import or path form; groups flattened above).
    for prefix in FORBIDDEN_STD {
        for (at, path) in imports.iter().filter(|(_, p)| p.contains(prefix)) {
            push(
                *at,
                format!(
                    "forbidden std path `{prefix}` (in `{path}`) — core is I/O-free; side effects belong to adapters"
                ),
            );
        }
        for at in find_all(&stripped, prefix) {
            if !in_use(at) {
                push(
                    at,
                    format!(
                        "forbidden std path `{prefix}` — core is I/O-free; side effects belong to adapters"
                    ),
                );
            }
        }
    }

    // tokio: import form via flattening, expression form via raw scan.
    let mut tokio_paths: Vec<(usize, String)> = imports
        .iter()
        .filter(|(_, p)| p.starts_with("tokio::"))
        .map(|(at, p)| (*at, (*p).to_string()))
        .collect();
    for at in find_all(&stripped, "tokio::") {
        if !in_use(at) {
            tokio_paths.push((at, path_from(&stripped, at)));
        }
    }
    for (at, path) in tokio_paths {
        if let Some(msg) = tokio_violation(&path, is_task_file) {
            push(at, msg);
        }
    }

    // mlua: banned everywhere except src/task.rs, and there only the
    // value-level items.
    if is_task_file {
        let mut mlua_paths: Vec<(usize, String)> = imports
            .iter()
            .filter(|(_, p)| p.starts_with("mlua::"))
            .map(|(at, p)| (*at, (*p).to_string()))
            .collect();
        for at in find_all(&stripped, "mlua::") {
            if !in_use(at) {
                mlua_paths.push((at, path_from(&stripped, at)));
            }
        }
        for (at, path) in mlua_paths {
            let item = path
                .strip_prefix("mlua::")
                .and_then(|r| r.split("::").next());
            if !item.is_some_and(|i| TASK_MLUA_ITEMS.contains(&i)) {
                push(
                    at,
                    format!(
                        "mlua item `{path}` — task.rs allows only the value-level set {TASK_MLUA_ITEMS:?}"
                    ),
                );
            }
        }
    } else {
        for at in find_all(&stripped, "mlua") {
            push(
                at,
                "mlua outside `src/task.rs` — task is core's only (data-level) mlua home"
                    .to_string(),
            );
        }
    }

    // agent_client_protocol: schema data types, four modules, no more.
    let mut acp_paths: Vec<(usize, String)> = imports
        .iter()
        .filter(|(_, p)| p.starts_with("agent_client_protocol"))
        .map(|(at, p)| (*at, (*p).to_string()))
        .collect();
    for at in find_all(&stripped, "agent_client_protocol") {
        if !in_use(at) {
            acp_paths.push((at, path_from(&stripped, at)));
        }
    }
    for (at, path) in acp_paths {
        if let Some(msg) = acp_violation(&path, module) {
            push(at, msg);
        }
    }
}

fn collect_rs_files(dir: &Path, root: &Path, out: &mut Vec<(String, String)>) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) => panic!("deps_guard: read_dir {}: {err}", dir.display()),
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => panic!("deps_guard: read_dir entry in {}: {err}", dir.display()),
        };
        let path = entry.path();
        let meta = match fs::metadata(&path) {
            Ok(meta) => meta,
            Err(err) => panic!("deps_guard: metadata {}: {err}", path.display()),
        };
        if meta.is_dir() {
            collect_rs_files(&path, root, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            let text = match fs::read_to_string(&path) {
                Ok(text) => text,
                Err(err) => panic!("deps_guard: read {}: {err}", path.display()),
            };
            let rel = match path.strip_prefix(root) {
                Ok(rel) => rel.to_string_lossy().replace('\\', "/"),
                Err(err) => panic!("deps_guard: strip_prefix {}: {err}", path.display()),
            };
            out.push((rel, text));
        }
    }
}

/// Synthetic snippets for the rule self-check: one violation per
/// rule, none of which compile in core today (the crate arrows block
/// them) — these pin the scanner itself against regressions and cover
/// the manifest-drift future where the compiler alone stops firing.
const RULE_PROBES: &[(&str, &str)] = &[
    ("src/text.rs", "use std::fs;\n"),
    ("src/text.rs", "use std::{env, io};\n"),
    ("src/text.rs", "use tokio::net::TcpStream;\n"),
    ("src/text.rs", "use tokio::{sync::mpsc, fs};\n"),
    ("src/text.rs", "use ptah_acp::Transport;\n"),
    ("src/text.rs", "fn f() { std::thread::sleep_ms(1); }\n"),
    ("src/text.rs", "use mlua::Lua;\n"),
    ("src/task.rs", "use mlua::Table;\n"),
    ("src/task.rs", "use tokio::task::spawn_local;\n"), // allowed — not a probe
    ("src/text.rs", "use tokio::task::spawn_local;\n"),
    (
        "src/text.rs",
        "use agent_client_protocol::schema::v1::ToolKind;\n",
    ),
    ("src/ports.rs", "use agent_client_protocol::Client;\n"),
    ("src/task.rs", "use mlua::{Function, Lua, MultiValue};\n"), // allowed
    ("src/ports.rs", "use tokio::sync::mpsc;\n"),                // allowed
];

#[test]
fn scanner_rules_fire_on_synthetic_probes() {
    let mut violations = Vec::new();
    for (rel, src) in RULE_PROBES {
        scan_file(rel, src, &mut violations);
    }
    let report = violations
        .iter()
        .map(Violation::render)
        .collect::<Vec<_>>()
        .join("\n");
    // Eleven of the fourteen probes are violations; the three marked
    // `allowed` must produce none.
    assert_eq!(violations.len(), 11, "probe report:\n{report}");
    let count = |file: &str| violations.iter().filter(|v| v.file == file).count();
    assert_eq!(count("src/text.rs"), 9, "probe report:\n{report}");
    assert_eq!(count("src/task.rs"), 1, "probe report:\n{report}");
    assert_eq!(count("src/ports.rs"), 1, "probe report:\n{report}");
}

#[test]
fn ptah_core_is_io_free_and_adapter_free() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let src = root.join("src");
    let mut files = Vec::new();
    collect_rs_files(&src, &root, &mut files);
    assert!(!files.is_empty(), "deps_guard: no sources under {src:?}");
    files.sort();

    let mut violations = Vec::new();
    for (rel, text) in &files {
        scan_file(rel, text, &mut violations);
    }
    assert!(
        violations.is_empty(),
        "dependency-direction violations in ptah-core (see the allowlist in \
         crates/ptah-core/tests/deps_guard.rs before widening anything):\n{}",
        violations
            .iter()
            .map(Violation::render)
            .collect::<Vec<_>>()
            .join("\n")
    );
}
