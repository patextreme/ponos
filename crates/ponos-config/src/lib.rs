//! Registry file I/O: TOML parsing and layer discovery for the agent
//! registry model ([`ponos_core::config`]).
//!
//! Project entries (`.ponos/config.toml`, discovered upward from the
//! invocation directory) override user entries (`$XDG_CONFIG_HOME/ponos/`
//! or `~/.config/ponos/config.toml`) wholesale per agent name.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ponos_core::config::{AgentSpec, ConfigError, Registry};
use ponos_core::ports::ConfigSource;

#[derive(Debug, Default, serde::Deserialize)]
struct RegistryFile {
    #[serde(default)]
    agents: BTreeMap<String, AgentSpec>,
}

/// Parse one registry layer's TOML contents into its agent map.
fn parse_layer(label: &str, contents: &str) -> Result<BTreeMap<String, AgentSpec>, ConfigError> {
    let file: RegistryFile = toml::from_str(contents).map_err(|e| ConfigError::Parse {
        label: label.into(),
        source: e.to_string(),
    })?;
    Ok(file.agents)
}

/// Parse a [`Registry`] from the contents of user and project config
/// files. Project entries replace user entries per agent name; other
/// entries merge.
///
/// A free function, not an inherent `Registry` method: TOML parsing is
/// this adapter's job and an inherent impl would be illegal across the
/// crate boundary (the model lives in `ponos-core`).
pub fn from_parts(user: Option<&str>, project: Option<&str>) -> Result<Registry, ConfigError> {
    let user = user.map(|c| parse_layer("user", c)).transpose()?;
    let project = project.map(|c| parse_layer("project", c)).transpose()?;
    Ok(Registry::from_layers(user, project))
}

/// Load from explicit file paths (missing files are fine).
pub fn load(user: Option<&Path>, project: Option<&Path>) -> Result<Registry, ConfigError> {
    let read = |p: Option<&Path>, label: &str| -> Result<Option<String>, ConfigError> {
        match p {
            None => Ok(None),
            Some(p) if !p.exists() => Ok(None),
            Some(p) => std::fs::read_to_string(p)
                .map(Some)
                .map_err(|source| ConfigError::Io {
                    label: label.into(),
                    source: source.to_string(),
                }),
        }
    };
    from_parts(
        read(user, "user")?.as_deref(),
        read(project, "project")?.as_deref(),
    )
}

/// Discover user (`$XDG_CONFIG_HOME/ponos` or `~/.config/ponos`) and
/// project (nearest ancestor `.ponos`, from the invocation directory)
/// registries.
pub fn discover(invocation_dir: &Path) -> Result<Registry, ConfigError> {
    let user = user_config_path();
    let project = find_project_config(invocation_dir);
    load(user.as_deref(), project.as_deref())
}

/// Filesystem-backed [`ConfigSource`]: TOML discovery and loading of the
/// user and project registry layers.
pub struct FsConfigSource;

impl ConfigSource for FsConfigSource {
    fn discover(&self, invocation_dir: &Path) -> Result<Registry, ConfigError> {
        discover(invocation_dir)
    }
}

/// `$XDG_CONFIG_HOME/ponos/config.toml` or `$HOME/.config/ponos/config.toml`.
pub fn user_config_path() -> Option<PathBuf> {
    let base = match std::env::var_os("XDG_CONFIG_HOME") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => {
            let home = std::env::var_os("HOME")?;
            PathBuf::from(home).join(".config")
        }
    };
    Some(base.join("ponos").join("config.toml"))
}

/// Nearest `.ponos/config.toml` in `start` or an ancestor directory.
pub fn find_project_config(start: &Path) -> Option<PathBuf> {
    let mut dir: &Path = start;
    loop {
        let candidate = dir.join(".ponos").join("config.toml");
        if candidate.is_file() {
            return Some(candidate);
        }
        {
            let parent = dir.parent()?;
            dir = parent
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const USER: &str = r#"
[agents.claude]
command = "claude-acp-user"
args = ["--old"]
env = { MODEL = "sonnet" }

[agents.shared]
command = "shared-bin"
"#;

    const PROJECT: &str = r#"
[agents.claude]
command = "claude-acp-project"
args = ["--new"]

[agents.gemini]
command = "gemini-acp"
"#;

    #[test]
    fn from_parts_merges_layers_with_project_winning() {
        let reg = from_parts(Some(USER), Some(PROJECT)).unwrap();
        let claude = reg.resolve_with("claude", &|_| None).unwrap();
        assert_eq!(claude.command, "claude-acp-project");
        assert!(claude.env.is_empty(), "user env must not leak: {claude:?}");
        assert_eq!(
            reg.resolve_with("gemini", &|_| None).unwrap().command,
            "gemini-acp"
        );
        assert_eq!(
            reg.resolve_with("shared", &|_| None).unwrap().command,
            "shared-bin"
        );
    }

    #[test]
    fn missing_files_are_ok() {
        let reg = load(None, Some(Path::new("/nonexistent/x.toml"))).unwrap();
        assert!(reg.agent_names().is_empty());
    }

    #[test]
    fn parse_error_is_labeled() {
        let err = from_parts(Some("not toml {{{"), None).unwrap_err();
        assert!(err.to_string().contains("user"), "{err}");
    }
}
