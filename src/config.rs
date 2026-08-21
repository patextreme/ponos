//! Agent registry configuration: TOML loading, project/user precedence,
//! `${VAR}` interpolation, and name resolution.
//!
//! Registries are TOML files mapping agent names to launch specs:
//!
//! ```toml
//! [agents.claude]
//! command = "npx"
//! args = ["-y", "@agentclientprotocol/claude-agent-acp"]
//! env = { ANTHROPIC_MODEL = "${MODEL}" }
//! ```
//!
//! Project entries (`.ponos/config.toml`, discovered upward from the
//! invocation directory) override user entries (`$XDG_CONFIG_HOME/ponos/`
//! or `~/.config/ponos/config.toml`) wholesale per agent name.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// A single agent launch specification, as written in a registry file.
///
/// Values are stored raw; `${VAR}` interpolation happens at resolve time.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct AgentSpec {
    /// Program to spawn.
    pub command: String,
    /// Extra arguments for the program.
    #[serde(default)]
    pub args: Vec<String>,
    /// Environment overrides merged over the inherited environment.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

impl AgentSpec {
    /// A spec with just a command.
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            args: Vec::new(),
            env: BTreeMap::new(),
        }
    }

    /// Interpolate `${VAR}` (unset → empty) across command, args, and env values.
    pub fn interpolate(&self, lookup: &dyn Fn(&str) -> Option<String>) -> AgentSpec {
        AgentSpec {
            command: interpolate(&self.command, lookup),
            args: self.args.iter().map(|a| interpolate(a, lookup)).collect(),
            env: self
                .env
                .iter()
                .map(|(k, v)| (k.clone(), interpolate(v, lookup)))
                .collect(),
        }
    }
}

/// Replace every `${VAR}` occurrence in `s` with the lookup result
/// (empty string when the variable is unset).
fn interpolate(s: &str, lookup: &dyn Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find('}') {
            Some(end) => {
                let var = &after[..end];
                if let Some(v) = lookup(var) {
                    out.push_str(&v)
                }
                rest = &after[end + 1..];
            }
            None => {
                // Unterminated: keep literally.
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

#[derive(Debug, Default, serde::Deserialize)]
struct RegistryFile {
    #[serde(default)]
    agents: BTreeMap<String, AgentSpec>,
}

/// A resolved registry: user entries merged with project-wins precedence.
#[derive(Debug, Default, Clone)]
pub struct Registry {
    agents: BTreeMap<String, AgentSpec>,
}

impl Registry {
    /// Parse a registry from the contents of user and project config files.
    /// Project entries replace user entries per agent name; other entries merge.
    pub fn from_parts(user: Option<&str>, project: Option<&str>) -> Result<Self, ConfigError> {
        let mut agents = BTreeMap::new();
        for (label, contents) in [("user", user), ("project", project)] {
            let Some(contents) = contents else { continue };
            let file: RegistryFile = toml::from_str(contents).map_err(|e| ConfigError::Parse {
                label: label.into(),
                source: e.to_string(),
            })?;
            for (name, spec) in file.agents {
                agents.insert(name, spec); // project processed last: wins wholesale
            }
        }
        Ok(Self { agents })
    }

    /// Load from explicit file paths (missing files are fine).
    pub fn load(user: Option<&Path>, project: Option<&Path>) -> Result<Self, ConfigError> {
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
        Self::from_parts(
            read(user, "user")?.as_deref(),
            read(project, "project")?.as_deref(),
        )
    }

    /// Discover user (`$XDG_CONFIG_HOME/ponos` or `~/.config/ponos`) and
    /// project (nearest ancestor `.ponos`, from the invocation directory)
    /// registries.
    pub fn discover(invocation_dir: &Path) -> Result<Self, ConfigError> {
        let user = user_config_path();
        let project = find_project_config(invocation_dir);
        Self::load(user.as_deref(), project.as_deref())
    }

    /// Resolve an agent by name, interpolating `${VAR}` from the process
    /// environment. Errors name the unresolved agent.
    pub fn resolve(&self, name: &str) -> Result<AgentSpec, ConfigError> {
        self.resolve_with(name, &|var| std::env::var(var).ok())
    }

    /// Like [`Registry::resolve`] with an explicit environment lookup
    /// (used by tests and callers that need deterministic interpolation).
    pub fn resolve_with(
        &self,
        name: &str,
        lookup: &dyn Fn(&str) -> Option<String>,
    ) -> Result<AgentSpec, ConfigError> {
        match self.agents.get(name) {
            Some(spec) => Ok(spec.interpolate(lookup)),
            None => Err(ConfigError::UnknownAgent {
                name: name.to_string(),
            }),
        }
    }

    /// Names of all registered agents.
    pub fn agent_names(&self) -> Vec<String> {
        self.agents.keys().cloned().collect()
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

#[derive(Debug)]
pub enum ConfigError {
    UnknownAgent { name: String },
    Parse { label: String, source: String },
    Io { label: String, source: String },
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::UnknownAgent { name } => write!(
                f,
                "unknown agent `{name}`: not found in the user or project registry"
            ),
            ConfigError::Parse { label, source } => {
                write!(f, "invalid {label} config: {source}")
            }
            ConfigError::Io { label, source } => {
                write!(f, "failed to read {label} config: {source}")
            }
        }
    }
}

impl std::error::Error for ConfigError {}

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

    fn registry() -> Registry {
        Registry::from_parts(Some(USER), Some(PROJECT)).unwrap()
    }

    #[test]
    fn project_overrides_user_wholesale() {
        let spec = registry().resolve_with("claude", &|_| None).unwrap();
        // Project definition wins; user fields (env MODEL) are NOT inherited.
        assert_eq!(spec.command, "claude-acp-project");
        assert_eq!(spec.args, vec!["--new"]);
        assert!(spec.env.is_empty(), "user env must not leak: {spec:?}");
    }

    #[test]
    fn disjoint_agents_merge() {
        let reg = registry();
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
    fn no_registry_errors_naming_agent() {
        let reg = Registry::from_parts(None, None).unwrap();
        let err = reg.resolve_with("claude", &|_| None).unwrap_err();
        assert!(
            err.to_string().contains("claude"),
            "error must name the agent: {err}"
        );
    }

    #[test]
    fn interpolate_set_unset_and_embedded() {
        let spec = AgentSpec {
            command: "${HOME}/bin/agent".into(),
            args: vec!["--key=${MISSING_KEY}".into(), "a${X}b".into()],
            env: [("M".to_string(), "${MODEL}".to_string())].into(),
        };
        let lookup = |v: &str| -> Option<String> {
            match v {
                "HOME" => Some("/home/pat".into()),
                "X" => Some("mid".into()),
                "MODEL" => Some("opus".into()),
                _ => None,
            }
        };
        let out = spec.interpolate(&lookup);
        assert_eq!(out.command, "/home/pat/bin/agent");
        assert_eq!(out.args[0], "--key=");
        assert_eq!(out.args[1], "amidb");
        assert_eq!(out.env["M"], "opus");
    }

    #[test]
    fn missing_files_are_ok() {
        let reg = Registry::load(None, Some(Path::new("/nonexistent/x.toml"))).unwrap();
        assert!(reg.agent_names().is_empty());
    }

    #[test]
    fn parse_error_is_labeled() {
        let err = Registry::from_parts(Some("not toml {{{"), None).unwrap_err();
        assert!(err.to_string().contains("user"), "{err}");
    }
}
