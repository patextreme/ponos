//! Relative module resolution for ponos scripts.
//!
//! Implements mlua's `Require` trait over the entry script's directory.
//! `require("./lib/util")` resolves `.luau`/`.lua`/`init.luau` files relative
//! to the requiring file; paths escaping the script tree and absolute paths
//! are rejected with a Lua error. Caching (same path → same module table) is
//! provided by mlua's loader cache keyed on the resolved path.

use std::path::{Component, Path, PathBuf};
use std::result::Result as StdResult;

use mlua::luau::{NavigateError, Require};
use mlua::{Function, Lua};

/// Lexically normalize a path: resolve `.`/`..` components without
/// touching the filesystem (`..` at the root pops, matching the
/// navigator). Shared by the runtime navigator and the static checker.
pub(crate) fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// True when `path` does not stay inside `root` (the script tree guard).
pub(crate) fn escapes(root: &Path, path: &Path) -> bool {
    !path.starts_with(root)
}

/// A literal require string is navigable only when explicitly relative.
pub(crate) fn is_relative_module(module: &str) -> bool {
    module.starts_with("./") || module.starts_with("../")
}

/// Resolve an already-joined module path to a physical file:
/// `<p>.luau`, `<p>.lua`, `<p>/init.luau`, `<p>/init.lua`.
fn resolve_file(path: &Path) -> Option<PathBuf> {
    for ext in ["luau", "lua"] {
        let candidate = path.with_extension(ext);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    for init in ["init.luau", "init.lua"] {
        let candidate = path.join(init);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Statically resolve a literal require argument (e.g. `"./lib/util"`)
/// exactly as the runtime navigator would, relative to the requiring
/// file's directory: the physical module file when a candidate exists,
/// `None` when none does. Callable without a `Lua` instance (used by the
/// `check` lints and the `run` pre-flight).
pub(crate) fn resolve_candidates(from_dir: &Path, module: &str) -> Option<PathBuf> {
    resolve_file(&normalize(&from_dir.join(module)))
}

/// A requirer rooted at the entry script's directory.
#[derive(Debug, Clone)]
pub struct ScriptRequirer {
    /// Absolute directory of the entry script; requires must stay inside.
    root: PathBuf,
    /// Absolute path (file or dir) the navigation currently points at.
    current: PathBuf,
}

impl ScriptRequirer {
    pub fn new(root: PathBuf) -> Self {
        Self {
            current: root.clone(),
            root,
        }
    }

    fn guard(&self, path: &Path) -> StdResult<(), NavigateError> {
        if !escapes(&self.root, path) {
            Ok(())
        } else {
            Err(NavigateError::Other(mlua::Error::runtime(format!(
                "require path escapes the script directory: {}",
                path.display()
            ))))
        }
    }
}

impl Require for ScriptRequirer {
    fn is_require_allowed(&self, chunk_name: &str) -> bool {
        chunk_name.starts_with('@')
    }

    fn reset(&mut self, chunk_name: &str) -> StdResult<(), NavigateError> {
        let raw = chunk_name
            .strip_prefix('@')
            .ok_or(NavigateError::NotFound)?;
        // Chunk line suffixes ("file.luau:12") are not module paths.
        let raw = raw.rsplit_once(':').map(|(p, _)| p).unwrap_or(raw);
        let path = normalize(Path::new(raw));
        let path = if path.is_absolute() {
            path
        } else {
            self.root.join(path)
        };
        self.guard(&path)?;
        self.current = path;
        Ok(())
    }

    fn jump_to_alias(&mut self, path: &str) -> StdResult<(), NavigateError> {
        // Non-relative require strings (aliases — and anything users hoped
        // would be an absolute path) are not supported.
        Err(NavigateError::Other(mlua::Error::runtime(format!(
            "require path is not relative to the script: `{path}` (only \"./\" and \"../\" paths are allowed)"
        ))))
    }

    fn to_parent(&mut self) -> StdResult<(), NavigateError> {
        let mut path = self.current.clone();
        if !path.pop() {
            return Err(NavigateError::NotFound);
        }
        let path = normalize(&path);
        self.guard(&path)?;
        self.current = path;
        Ok(())
    }

    fn to_child(&mut self, name: &str) -> StdResult<(), NavigateError> {
        let path = normalize(&self.current.join(name));
        self.guard(&path)?;
        // A child that is neither a module nor a directory cannot be part of
        // any deeper resolution: fail with an error naming the path (per the
        // scripting spec) instead of a generic not-found.
        if resolve_file(&path).is_none() && !path.is_dir() {
            return Err(NavigateError::Other(mlua::Error::runtime(format!(
                "module not found: {}",
                path.display()
            ))));
        }
        self.current = path;
        Ok(())
    }

    fn has_module(&self) -> bool {
        resolve_file(&self.current).is_some()
    }

    fn cache_key(&self) -> String {
        resolve_file(&self.current)
            .map(|p| p.display().to_string())
            .unwrap_or_default()
    }

    fn has_config(&self) -> bool {
        false
    }

    fn config(&self) -> std::io::Result<Vec<u8>> {
        Ok(Vec::new())
    }

    fn loader(&self, lua: &Lua) -> mlua::Result<Function> {
        let path = resolve_file(&self.current).ok_or_else(|| {
            mlua::Error::runtime(format!("module not found: {}", self.current.display()))
        })?;
        let source = std::fs::read_to_string(&path).map_err(|e| {
            mlua::Error::runtime(format!("cannot read module {}: {e}", path.display()))
        })?;
        lua.load(source)
            .set_name(format!("@{}", path.display()))
            .into_function()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ponos-req-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("lib")).unwrap();
        std::fs::create_dir_all(dir.join("lib/sub")).unwrap();
        std::fs::write(dir.join("main.luau"), "return 1").unwrap();
        std::fs::write(dir.join("lib/util.luau"), "return 2").unwrap();
        std::fs::write(dir.join("lib/sub/init.luau"), "return 3").unwrap();
        dir
    }

    #[test]
    fn navigates_sibling_module() {
        let dir = tmp();
        let mut req = ScriptRequirer::new(dir.clone());
        req.reset(&format!("@{}/main.luau", dir.display())).unwrap();
        req.to_parent().unwrap();
        req.to_child("lib").unwrap();
        req.to_child("util").unwrap();
        assert!(req.has_module());
        assert!(req.cache_key().ends_with("lib/util.luau"));
    }

    #[test]
    fn resolves_init_files() {
        let dir = tmp();
        let mut req = ScriptRequirer::new(dir.clone());
        req.reset(&format!("@{}/main.luau", dir.display())).unwrap();
        req.to_parent().unwrap();
        req.to_child("lib").unwrap();
        req.to_child("sub").unwrap();
        assert!(req.has_module());
        assert!(req.cache_key().ends_with("lib/sub/init.luau"));
    }

    #[test]
    fn missing_module_names_the_path() {
        let dir = tmp();
        let mut req = ScriptRequirer::new(dir.clone());
        req.reset(&format!("@{}/main.luau", dir.display())).unwrap();
        req.to_parent().unwrap();
        req.to_child("lib").unwrap();
        // Navigating into a nonexistent module errors with the path named.
        let err = req.to_child("nope").unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("nope"), "{msg}");
        assert!(!req.has_module());
    }

    #[test]
    fn escaping_paths_rejected() {
        let dir = tmp();
        let mut req = ScriptRequirer::new(dir.clone());
        req.reset(&format!("@{}/main.luau", dir.display())).unwrap();
        req.to_parent().unwrap(); // -> root itself is allowed
        let err = req.to_parent().unwrap_err(); // -> escapes the script root
        assert!(matches!(err, NavigateError::Other(_)), "{err:?}");
    }

    #[test]
    fn absolute_require_strings_rejected() {
        let mut req = ScriptRequirer::new(tmp());
        let err = req.jump_to_alias("/etc/passwd").unwrap_err();
        assert!(matches!(err, NavigateError::Other(_)), "{err:?}");
    }

    // ------------------------------------------------------------------
    // Pure helpers (shared with the static checker)
    // ------------------------------------------------------------------

    #[test]
    fn pure_helpers_resolve_candidates() {
        let dir = tmp();
        let main = dir.join("main.luau");
        assert_eq!(
            resolve_candidates(main.parent().unwrap(), "./lib/util"),
            Some(dir.join("lib/util.luau"))
        );
        assert_eq!(
            resolve_candidates(main.parent().unwrap(), "./lib/sub"),
            Some(dir.join("lib/sub/init.luau"))
        );
        assert_eq!(
            resolve_candidates(main.parent().unwrap(), "./lib/nope"),
            None
        );
        // Parent-relative from a nested directory.
        let nested = dir.join("lib/sub");
        assert_eq!(
            resolve_candidates(&nested, "../util"),
            Some(dir.join("lib/util.luau"))
        );
    }

    #[test]
    fn pure_helpers_normalize_and_escape() {
        let root = Path::new("/script/tree");
        assert!(!escapes(root, &normalize(&root.join("./lib/../lib/x"))));
        assert!(escapes(root, &normalize(&root.join("../../outside"))));
        // `..` at the root pops; anything landing outside the root escapes.
        assert!(escapes(root, &normalize(&root.join(".."))));
        assert!(is_relative_module("./x"));
        assert!(is_relative_module("../x"));
        assert!(!is_relative_module("x"));
        assert!(!is_relative_module("@alias/x"));
        assert!(!is_relative_module("/abs/x"));
    }
}
