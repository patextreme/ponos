//! Ports and policies: the seams where adapters plug into the core.
//!
//! Today this module holds the [`EventSink`] port and the headless
//! [`InteractionPolicy`]. The remaining ports (config source, agent
//! transport) land here as the restructure proceeds.

use std::path::Path;

use agent_client_protocol::schema::v1::{
    PermissionOption, PermissionOptionId, PermissionOptionKind,
};

use crate::core::config::{ConfigError, Registry};
use crate::core::events::SessionEvent;

/// Where the agent registry comes from. The TOML/fs loader implements it
/// today (user + project layers, project-wins precedence); the port is
/// the seam for other sources (embedded, remote) without touching
/// callers.
pub trait ConfigSource: Send + Sync {
    /// Discover and load the registry for an invocation directory.
    fn discover(&self, invocation_dir: &Path) -> Result<Registry, ConfigError>;
}

/// Where session events go. The session driver folds wire updates and
/// emits structured [`SessionEvent`]s through this port; the terminal
/// renderer implements it today, and a TUI or structured logger can take
/// the same seam without touching the driver.
pub trait EventSink: Send + Sync {
    /// One structured event, attributed to a session by its label.
    fn emit(&self, label: &str, event: SessionEvent);
    /// A script-initiated log line (`ponos.log`): not a session event
    /// and not suppressed by `--quiet`.
    fn script_log(&self, message: &str);
}

/// Decisions for agent→client requests that would otherwise need a user
/// present. ponos runs headless; the policy is the exact seam a TUI (or
/// any interactive front end) needs to make permissions interactive
/// without touching transport code.
pub trait InteractionPolicy: Send + Sync {
    /// The option id to answer `session/request_permission` with, or
    /// `None` when the offer has no allow option to select (the adapter
    /// then answers method-not-found).
    fn select_permission(&self, options: &[PermissionOption]) -> Option<PermissionOptionId>;
}

/// The headless posture: prefer `AllowAlways`, else the first other
/// allow-kind option (documented in the README; choosing `AllowAlways`
/// may let the agent persist an allow rule in its own settings beyond
/// the run).
pub struct HeadlessPolicy;

impl InteractionPolicy for HeadlessPolicy {
    fn select_permission(&self, options: &[PermissionOption]) -> Option<PermissionOptionId> {
        select_allow_option(options)
    }
}

/// Pick the option to answer a permission request with: the first
/// `AllowAlways` when offered, otherwise the first other allow-kind
/// option. `None` when the offer has no allow option at all.
pub(crate) fn select_allow_option(options: &[PermissionOption]) -> Option<PermissionOptionId> {
    options
        .iter()
        .find(|o| matches!(o.kind, PermissionOptionKind::AllowAlways))
        .or_else(|| {
            options
                .iter()
                .find(|o| matches!(o.kind, PermissionOptionKind::AllowOnce))
        })
        .map(|o| o.option_id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn option(id: &str, kind: PermissionOptionKind) -> PermissionOption {
        PermissionOption::new(id.to_string(), "label", kind)
    }

    #[test]
    fn allow_selection_prefers_allow_always() {
        let options = vec![
            option("allow_once", PermissionOptionKind::AllowOnce),
            option("allow_always", PermissionOptionKind::AllowAlways),
        ];
        assert_eq!(
            select_allow_option(&options),
            Some(PermissionOptionId::new("allow_always"))
        );
    }

    #[test]
    fn allow_selection_falls_back_to_any_allow_kind() {
        let options = vec![
            option("reject_once", PermissionOptionKind::RejectOnce),
            option("allow_once", PermissionOptionKind::AllowOnce),
        ];
        assert_eq!(
            select_allow_option(&options),
            Some(PermissionOptionId::new("allow_once"))
        );
    }

    #[test]
    fn allow_selection_reject_only_offer_gets_method_not_found() {
        let options = vec![
            option("reject_once", PermissionOptionKind::RejectOnce),
            option("reject_always", PermissionOptionKind::RejectAlways),
        ];
        assert_eq!(select_allow_option(&options), None);
        assert_eq!(select_allow_option(&[]), None);
    }

    #[test]
    fn headless_policy_implements_the_selection_rule() {
        let policy = HeadlessPolicy;
        let options = vec![
            option("reject_once", PermissionOptionKind::RejectOnce),
            option("allow_once", PermissionOptionKind::AllowOnce),
            option("allow_always", PermissionOptionKind::AllowAlways),
        ];
        let dyn_policy: &dyn InteractionPolicy = &policy;
        assert_eq!(
            dyn_policy.select_permission(&options),
            Some(PermissionOptionId::new("allow_always"))
        );
        assert_eq!(dyn_policy.select_permission(&[]), None);
    }
}
