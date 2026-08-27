//! Relative module resolution for ptah scripts.
//!
//! Implements mlua's `Require` trait relative to the entry script's
//! directory. `require("./lib/util")` resolves `.luau`/`.lua`/`init.luau`
//! files relative to the requiring file, with no boundary: relative paths
//! may walk out of the entry script's directory to anywhere on disk.
//! Non-relative require strings (absolute paths, bare names, aliases) are
//! rejected with a Lua error. Caching (same path → same module table) is
//! provided by mlua's loader cache keyed on the resolved path.

use std::path::{Component, Path, PathBuf};
use std::result::Result as StdResult;

use mlua::luau::{NavigateError, Require};
use mlua::{Function, Lua};

/// Lexically normalize a path: resolve `.`/`..` components without
/// touching the filesystem (`..` at the root pops, matching the
/// navigator). Shared by the runtime navigator and the static checker.
fn normalize(path: &Path) -> PathBuf {
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

/// A requirer rooted at the entry script's directory.
#[derive(Debug, Clone)]
pub struct ScriptRequirer {
    /// Absolute directory of the entry script; relative chunk names are
    /// joined onto it.
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
        let raw = raw.rsplit_once(':').map_or(raw, |(p, _)| p);
        let path = normalize(Path::new(raw));
        let path = if path.is_absolute() {
            path
        } else {
            self.root.join(path)
        };
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
        self.current = path;
        Ok(())
    }

    fn to_child(&mut self, name: &str) -> StdResult<(), NavigateError> {
        let path = normalize(&self.current.join(name));
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
        let dir = std::env::temp_dir().join(format!("ptah-req-{}", std::process::id()));
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
    fn navigates_out_of_the_script_directory() {
        // Two sibling trees under one parent: workflow/main.luau requires
        // ../shared/helper, which walks out of the script root.
        let base = std::env::temp_dir().join(format!("ptah-req-cross-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("workflow")).unwrap();
        std::fs::create_dir_all(base.join("shared")).unwrap();
        std::fs::write(base.join("workflow/main.luau"), "return 1").unwrap();
        std::fs::write(base.join("shared/helper.luau"), "return 2").unwrap();

        let mut req = ScriptRequirer::new(base.join("workflow"));
        req.reset(&format!("@{}/workflow/main.luau", base.display()))
            .unwrap();
        // require("../shared/helper") from workflow/main.luau
        req.to_parent().unwrap(); // main.luau -> workflow/
        req.to_parent().unwrap(); // workflow/ -> base/ (outside the root)
        req.to_child("shared").unwrap();
        req.to_child("helper").unwrap();
        assert!(req.has_module());
        assert!(req.cache_key().ends_with("shared/helper.luau"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn absolute_require_strings_rejected() {
        let mut req = ScriptRequirer::new(tmp());
        let err = req.jump_to_alias("/etc/passwd").unwrap_err();
        assert!(matches!(err, NavigateError::Other(_)), "{err:?}");
    }

    // ------------------------------------------------------------------
    // Pure helpers (the navigator's directory rules)
    // ------------------------------------------------------------------

    #[test]
    fn pure_helpers_normalize() {
        assert_eq!(
            normalize(Path::new("/base/dir/./lib/../lib/x")),
            Path::new("/base/dir/lib/x")
        );
        // `..` at the root pops.
        assert_eq!(normalize(Path::new("/..")), Path::new("/"));
    }
}
