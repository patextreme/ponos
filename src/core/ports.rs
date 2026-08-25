//! Ports and policies: the seams where adapters plug into the core.
//!
//! Today this module holds the headless interaction policy — the
//! decision rule for agent→client `session/request_permission` requests
//! — and the [`EventSink`] port. The remaining ports (config source,
//! agent transport) land here as the restructure proceeds.

use agent_client_protocol::schema::v1::{PermissionOption, PermissionOptionId, PermissionOptionKind};

use crate::core::events::SessionEvent;

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
}
