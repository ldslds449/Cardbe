use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkSettingsField {
    ListenPort,
    DirectAddresses,
    RelayUrls,
    DiscoveryUrls,
}

/// Service errors stay independent of Tauri and localization.
#[derive(Debug)]
pub enum DomainError {
    TaskNotFound,
    BoardNotFound,
    ColumnNotFound,
    BoardNameRequired,
    LastBoardRequired,
    StaleBoardRequest,
    PermissionDenied,
    NothingToUndo,
    NothingToRedo,
    NoteNotFound,
    TemplateNotFound,
    RelationshipSelf,
    RelationshipDuplicate,
    RelationshipNotFound,
    FlowTabNotFound,
    LastFlowTabRequired,
    InvalidArgument,
    NetworkSettingsInvalid { field: NetworkSettingsField },
    InviteNotFound,
    InviteInvalid,
    InviteAlreadyJoined,
    DeviceRequestNotFound,
    SyncConflictNotFound,
    LanUnavailable,
    ShareLinkDisabled,
    ShareSnapshotInvalid,
    ShareLimitExceeded,
    ShareExpirationInvalid,
    McpPortUnavailable,
    McpClientNotFound,
    Internal(String),
}

impl From<&str> for DomainError {
    fn from(source: &str) -> Self {
        Self::Internal(source.into())
    }
}

impl From<String> for DomainError {
    fn from(source: String) -> Self {
        Self::Internal(source)
    }
}

impl DomainError {
    pub fn repository(source: Box<dyn std::error::Error>) -> Self {
        match source.downcast::<Self>() {
            Ok(error) => *error,
            Err(source) => Self::Internal(source.to_string()),
        }
    }
}

impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TaskNotFound => f.write_str("Task not found"),
            Self::BoardNotFound => f.write_str("Board not found"),
            Self::ColumnNotFound => f.write_str("Column not found"),
            Self::BoardNameRequired => f.write_str("Board name cannot be empty"),
            Self::LastBoardRequired => f.write_str("At least one board is required"),
            Self::StaleBoardRequest => f.write_str("Stale board request"),
            Self::PermissionDenied => f.write_str("This shared board is read-only"),
            Self::NothingToUndo => f.write_str("There is nothing to undo"),
            Self::NothingToRedo => f.write_str("There is nothing to redo"),
            Self::NoteNotFound => f.write_str("Note not found"),
            Self::TemplateNotFound => f.write_str("Template not found"),
            Self::RelationshipSelf => f.write_str("A task cannot relate to itself"),
            Self::RelationshipDuplicate => f.write_str("Relationship already exists"),
            Self::RelationshipNotFound => f.write_str("Relationship not found"),
            Self::FlowTabNotFound => f.write_str("Canvas tab not found"),
            Self::LastFlowTabRequired => f.write_str("At least one canvas tab is required"),
            Self::InvalidArgument => f.write_str("Invalid command arguments"),
            Self::NetworkSettingsInvalid { field } => {
                write!(f, "Invalid network setting: {field:?}")
            }
            Self::InviteNotFound => f.write_str("Invitation not found"),
            Self::InviteInvalid => f.write_str("Invalid invitation"),
            Self::InviteAlreadyJoined => f.write_str("Invitation has already been joined"),
            Self::DeviceRequestNotFound => f.write_str("Device request not found"),
            Self::SyncConflictNotFound => f.write_str("No saved sync conflict"),
            Self::LanUnavailable => f.write_str("No private LAN connection available"),
            Self::ShareLinkDisabled => f.write_str("Share link is disabled"),
            Self::ShareSnapshotInvalid => f.write_str("Invalid share snapshot"),
            Self::ShareLimitExceeded => f.write_str("Share limit exceeded"),
            Self::ShareExpirationInvalid => f.write_str("Invalid share expiration"),
            Self::McpPortUnavailable => f.write_str("MCP server port is unavailable"),
            Self::McpClientNotFound => f.write_str("MCP client not found"),
            Self::Internal(source) => f.write_str(source),
        }
    }
}

impl std::error::Error for DomainError {}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "code", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CommandError {
    ShareUnavailable,
    TaskNotFound,
    BoardNotFound,
    ColumnNotFound,
    BoardNameRequired,
    LastBoardRequired,
    StaleBoardRequest,
    PermissionDenied,
    NothingToUndo,
    NothingToRedo,
    NoteNotFound,
    NoteContentRequired,
    TemplateNotFound,
    TemplateNameRequired,
    TaskTitleRequired,
    RelationshipSelf,
    RelationshipDuplicate,
    RelationshipNotFound,
    FlowTabNotFound,
    LastFlowTabRequired,
    FocusNotFound,
    InvalidArgument,
    NetworkSettingsInvalid {
        field: NetworkSettingsField,
    },
    InvalidImport,
    UnsupportedBackupVersion,
    BoardNameInvalid,
    InviteNotFound,
    InviteDisabled,
    InviteInvalid,
    InviteAlreadyJoined,
    DeviceRequestNotFound,
    SyncConflictNotFound,
    LanUnavailable,
    ShareLinkDisabled,
    ShareSnapshotInvalid,
    ShareLimitExceeded,
    ShareExpirationInvalid,
    CalendarFontNotFound,
    ExternalUrlInvalid,
    ShortcutUnavailable,
    ShortcutAlreadyAssigned,
    UpdateRequestFailed,
    UpdateReleaseDataInvalid,
    ShareAccessRevoked,
    ShareAccessTerminated,
    ShareInvitationDeleted,
    ShareApprovalRequired {
        device_id: String,
    },
    ShareSyncConflict,
    ShareFailed,
    IrohDisabled,
    PluginPermissionDenied {
        plugin_id: String,
        permission: String,
    },
    PluginExecutionFailed {
        plugin_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<PluginFailureReason>,
    },
    PluginPackageInvalid {
        reason: PluginPackageIssue,
        /// Validated manifest setting key, never other package text.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        setting: Option<String>,
        /// Validated `x.y.z` API version declared by the package.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        api_version: Option<String>,
    },
    PluginUpdateNotNewer,
    PluginUpdateBusy,
    PluginUpdateStale,
    McpPortUnavailable,
    McpClientNotFound,
    PluginError {
        plugin_id: String,
        plugin_error: PluginUserError,
    },
    ResourceUsageUnavailable,
    InternalError,
}

#[derive(Debug)]
pub enum ShareError {
    IrohDisabled,
    ApprovalRequired { device_id: String },
    AccessRevoked,
    AccessTerminated,
    InvitationDeleted,
    InvitationDisabled,
    SyncConflict,
    Domain(DomainError),
    Internal(String),
}
impl fmt::Display for ShareError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Internal(source) => f.write_str(source),
            _ => write!(f, "{self:?}"),
        }
    }
}
impl From<DomainError> for ShareError {
    fn from(error: DomainError) -> Self {
        Self::Domain(error)
    }
}
impl From<String> for ShareError {
    fn from(source: String) -> Self {
        Self::Internal(source)
    }
}
impl From<&str> for ShareError {
    fn from(source: &str) -> Self {
        Self::Internal(source.into())
    }
}
impl From<ShareError> for CommandError {
    fn from(error: ShareError) -> Self {
        match error {
            ShareError::ApprovalRequired { device_id } => Self::ShareApprovalRequired { device_id },
            ShareError::IrohDisabled => Self::IrohDisabled,
            ShareError::AccessRevoked => Self::ShareAccessRevoked,
            ShareError::AccessTerminated => Self::ShareAccessTerminated,
            ShareError::InvitationDeleted => Self::ShareInvitationDeleted,
            ShareError::InvitationDisabled => Self::InviteDisabled,
            ShareError::SyncConflict => Self::ShareSyncConflict,
            ShareError::Domain(error) => error.into(),
            ShareError::Internal(source) => Self::internal(source),
        }
    }
}

impl CommandError {
    pub fn internal(source: impl fmt::Display) -> Self {
        // Reuse diagnostic sanitization before persisting backend details.
        let detail = crate::commands::diagnostics::redact_diagnostic_logs(source.to_string());
        log::error!(target: "command", "{detail}");
        Self::InternalError
    }

    pub fn plugin_package(reason: PluginPackageIssue) -> Self {
        Self::PluginPackageInvalid {
            reason,
            setting: None,
            api_version: None,
        }
    }

    pub fn repository(source: Box<dyn std::error::Error>) -> Self {
        match source.downcast::<DomainError>() {
            Ok(error) => (*error).into(),
            Err(source) => Self::internal(source),
        }
    }
}

impl From<String> for CommandError {
    fn from(source: String) -> Self {
        Self::internal(source)
    }
}

impl From<DomainError> for CommandError {
    fn from(error: DomainError) -> Self {
        match error {
            DomainError::TaskNotFound => Self::TaskNotFound,
            DomainError::BoardNotFound => Self::BoardNotFound,
            DomainError::ColumnNotFound => Self::ColumnNotFound,
            DomainError::BoardNameRequired => Self::BoardNameRequired,
            DomainError::LastBoardRequired => Self::LastBoardRequired,
            DomainError::StaleBoardRequest => Self::StaleBoardRequest,
            DomainError::PermissionDenied => Self::PermissionDenied,
            DomainError::NothingToUndo => Self::NothingToUndo,
            DomainError::NothingToRedo => Self::NothingToRedo,
            DomainError::NoteNotFound => Self::NoteNotFound,
            DomainError::TemplateNotFound => Self::TemplateNotFound,
            DomainError::RelationshipSelf => Self::RelationshipSelf,
            DomainError::RelationshipDuplicate => Self::RelationshipDuplicate,
            DomainError::RelationshipNotFound => Self::RelationshipNotFound,
            DomainError::FlowTabNotFound => Self::FlowTabNotFound,
            DomainError::LastFlowTabRequired => Self::LastFlowTabRequired,
            DomainError::InvalidArgument => Self::InvalidArgument,
            DomainError::NetworkSettingsInvalid { field } => Self::NetworkSettingsInvalid { field },
            DomainError::InviteNotFound => Self::InviteNotFound,
            DomainError::InviteInvalid => Self::InviteInvalid,
            DomainError::InviteAlreadyJoined => Self::InviteAlreadyJoined,
            DomainError::DeviceRequestNotFound => Self::DeviceRequestNotFound,
            DomainError::SyncConflictNotFound => Self::SyncConflictNotFound,
            DomainError::LanUnavailable => Self::LanUnavailable,
            DomainError::ShareLinkDisabled => Self::ShareLinkDisabled,
            DomainError::ShareSnapshotInvalid => Self::ShareSnapshotInvalid,
            DomainError::ShareLimitExceeded => Self::ShareLimitExceeded,
            DomainError::ShareExpirationInvalid => Self::ShareExpirationInvalid,
            DomainError::McpPortUnavailable => Self::McpPortUnavailable,
            DomainError::McpClientNotFound => Self::McpClientNotFound,
            DomainError::Internal(source) => Self::internal(source),
        }
    }
}

/// Plugin params are shallow, bounded JSON scalars. Hosts must approve public
/// fields against each plugin's schema; credentials and exceptions are forbidden.
#[derive(Debug, Clone, Serialize)]
pub struct PluginUserError {
    code: String,
    params: serde_json::Map<String, serde_json::Value>,
}

impl<'de> Deserialize<'de> for PluginUserError {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct PublicError {
            code: String,
            params: serde_json::Map<String, serde_json::Value>,
        }
        let value = PublicError::deserialize(deserializer)?;
        Self::new(value.code, value.params)
            .ok_or_else(|| serde::de::Error::custom("Invalid plugin error"))
    }
}
impl PluginUserError {
    pub fn new(code: String, params: serde_json::Map<String, serde_json::Value>) -> Option<Self> {
        if code.is_empty()
            || code.len() > 64
            || !code
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
            || params.len() > 16
            || params
                .values()
                .any(|value| value.is_array() || value.is_object())
        {
            return None;
        }
        let error = Self { code, params };
        (serde_json::to_vec(&error).ok()?.len() <= 4096).then_some(error)
    }
}

/// Independent of invoke, so a future scheduler can use the same host errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PluginFailureReason {
    InvalidComponent,
    NetworkFailed,
    ResourceLimit,
    Timeout,
    Cancelled,
    WasmTrap,
    InvalidGuestError,
    InvalidCredential,
}

/// Why a plugin package was rejected during installation or update.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PluginPackageIssue {
    ArchiveTooLarge,
    ArchiveUnreadable,
    ArchiveTooManyEntries,
    UnsafePath,
    ManifestMissing,
    ManifestDuplicate,
    ComponentMissing,
    ManifestTooLarge,
    ComponentTooLarge,
    ManifestMalformed,
    InvalidId,
    InvalidVersion,
    UnsupportedApiVersion,
    UnsupportedStorageSchema,
    UnsupportedManifestVersion,
    InvalidDomains,
    InvalidCodes,
    LegacyCredentials,
    InvalidSettings,
    InvalidCredentialRules,
    InvalidComponent,
    DownloadUrlInvalid,
    DownloadFailed,
    PackageLimit,
}

#[allow(dead_code)]
#[derive(Debug)]
pub enum PluginHostError {
    RuntimeFailed {
        plugin_id: String,
        reason: PluginFailureReason,
    },
    PermissionDenied {
        plugin_id: String,
        permission: String,
    },
    ExecutionFailed {
        plugin_id: String,
        source: String,
    },
    User {
        plugin_id: String,
        error: PluginUserError,
    },
}

impl From<PluginHostError> for CommandError {
    fn from(error: PluginHostError) -> Self {
        match error {
            PluginHostError::RuntimeFailed { plugin_id, reason } => Self::PluginExecutionFailed {
                plugin_id,
                reason: Some(reason),
            },
            PluginHostError::PermissionDenied {
                plugin_id,
                permission,
            } => Self::PluginPermissionDenied {
                plugin_id,
                permission,
            },
            PluginHostError::ExecutionFailed { plugin_id, source } => {
                Self::internal(source);
                Self::PluginExecutionFailed {
                    plugin_id,
                    reason: None,
                }
            }
            PluginHostError::User { plugin_id, error } => Self::PluginError {
                plugin_id,
                plugin_error: error,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn boundary_serializes_codes_and_hides_internal_details() {
        assert_eq!(
            serde_json::to_value(CommandError::from(PluginHostError::RuntimeFailed {
                plugin_id: "fixture".into(),
                reason: PluginFailureReason::WasmTrap
            }))
            .unwrap(),
            json!({"code":"PLUGIN_EXECUTION_FAILED", "plugin_id":"fixture", "reason":"WASM_TRAP"})
        );
        let error = ShareError::from(DomainError::repository(Box::new(
            DomainError::BoardNotFound,
        )));
        assert_eq!(
            serde_json::to_value(CommandError::from(error)).unwrap(),
            json!({"code": "BOARD_NOT_FOUND"})
        );
        assert_eq!(
            serde_json::to_value(CommandError::from(ShareError::Internal(
                "password=secret".into()
            )))
            .unwrap(),
            json!({"code": "INTERNAL_ERROR"})
        );
        assert_eq!(
            serde_json::to_value(CommandError::from(DomainError::TaskNotFound)).unwrap(),
            json!({"code": "TASK_NOT_FOUND"})
        );
        let error =
            CommandError::repository(Box::new(std::io::Error::other("SQLite password=secret")));
        assert_eq!(
            serde_json::to_value(error).unwrap(),
            json!({"code": "INTERNAL_ERROR"})
        );
        let error = CommandError::from(PluginHostError::ExecutionFailed {
            plugin_id: "github-sync".into(),
            source: "token=secret".into(),
        });
        assert_eq!(
            serde_json::to_value(error).unwrap(),
            json!({"code": "PLUGIN_EXECUTION_FAILED", "plugin_id": "github-sync"})
        );
        let error = CommandError::from(PluginHostError::PermissionDenied {
            plugin_id: "github-sync".into(),
            permission: "network".into(),
        });
        assert_eq!(
            serde_json::to_value(error).unwrap(),
            json!({"code": "PLUGIN_PERMISSION_DENIED", "plugin_id": "github-sync", "permission": "network"})
        );
    }

    #[test]
    fn plugin_params_are_bounded_and_namespaced() {
        let params = json!({"repository": "owner/repo"})
            .as_object()
            .unwrap()
            .clone();
        let error = PluginUserError::new("REPOSITORY_NOT_FOUND".into(), params).unwrap();
        let value = serde_json::to_value(CommandError::from(PluginHostError::User {
            plugin_id: "github-sync".into(),
            error,
        }))
        .unwrap();
        assert_eq!(value["code"], "PLUGIN_ERROR");
        assert_eq!(value["plugin_error"]["params"]["repository"], "owner/repo");
        assert!(PluginUserError::new(
            "BAD".into(),
            json!({"nested": {"secret": true}})
                .as_object()
                .unwrap()
                .clone()
        )
        .is_none());
        assert!(PluginUserError::new(
            "BAD".into(),
            json!({"large": "x".repeat(4096)})
                .as_object()
                .unwrap()
                .clone()
        )
        .is_none());
    }
}
#[cfg(test)]
mod plugin_persistence_error_tests {
    use super::*;
    #[test]
    fn persisted_plugin_errors_revalidate_public_params() {
        assert!(serde_json::from_str::<PluginUserError>(
            r#"{"code":"FAILED","params":{"nested":{"secret":"no"}}}"#
        )
        .is_err());
        assert!(
            serde_json::from_str::<PluginUserError>(r#"{"code":"invalid","params":{}}"#).is_err()
        );
        let error = PluginUserError::new("FAILED".into(), serde_json::Map::new()).unwrap();
        assert!(
            serde_json::from_str::<PluginUserError>(&serde_json::to_string(&error).unwrap())
                .is_ok()
        );
    }
}
