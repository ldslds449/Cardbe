use crate::errors::CommandError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(untagged)]
pub enum LocalizedText {
    #[default]
    Empty,
    Text(String),
    Locales(BTreeMap<String, String>),
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SettingOption {
    pub value: String,
    pub label: LocalizedText,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SettingField {
    pub key: String,
    pub label: LocalizedText,
    #[serde(default)]
    pub description: LocalizedText,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub options: Vec<SettingOption>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub default: Option<serde_json::Value>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum CredentialDestination {
    Domain { domain: String },
    BaseUrl { setting: String },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HttpCredentialRule {
    pub destination: CredentialDestination,
    pub path_prefix: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bearer_secret: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<HttpAuth>,
}
impl<'de> Deserialize<'de> for HttpCredentialRule {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        fn present<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
            deserializer: D,
        ) -> Result<Option<T>, D::Error> {
            T::deserialize(deserializer).map(Some)
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Rule {
            destination: CredentialDestination,
            path_prefix: String,
            #[serde(default, deserialize_with = "present")]
            bearer_secret: Option<Option<String>>,
            #[serde(default, deserialize_with = "present")]
            auth: Option<Option<HttpAuth>>,
        }
        let rule = Rule::deserialize(deserializer)?;
        if rule.auth.is_some() && rule.bearer_secret.is_some() {
            return Err(serde::de::Error::custom("Conflicting credential formats"));
        }
        Ok(Self {
            destination: rule.destination,
            path_prefix: rule.path_prefix,
            bearer_secret: rule.bearer_secret.flatten(),
            auth: rule.auth.flatten(),
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum HttpAuth {
    Bearer { secret: String },
    Header { name: String, secret: String },
}
impl HttpAuth {
    pub fn secret(&self) -> &str {
        match self {
            Self::Bearer { secret } | Self::Header { secret, .. } => secret,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PluginPackage {
    pub id: String,
    pub name: LocalizedText,
    pub version: String,
    #[serde(default = "legacy_manifest_version")]
    pub manifest_version: u32,
    pub api_version: String,
    pub storage_schema_version: u32,
    #[serde(default)]
    pub error_codes: Vec<String>,
    #[serde(default)]
    pub log_codes: Vec<String>,
    #[serde(default)]
    pub description: LocalizedText,
    #[serde(default)]
    pub domains: Vec<String>,
    #[serde(default)]
    pub allow_custom_domains: bool,
    // None preserves legacy auth; an explicit empty list disables automatic auth.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http_credentials: Option<Vec<HttpCredentialRule>>,
    #[serde(default)]
    pub settings: Vec<SettingField>,
}
fn legacy_manifest_version() -> u32 {
    1
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PendingDomain {
    pub domain: String,
    pub run_id: String,
    pub trigger: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PluginInstance {
    pub id: String,
    pub plugin_id: String,
    pub name: String,
    pub board_id: i64,
    pub config: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    pub secret_fields: Vec<String>,
    pub allowed_domains: Vec<String>,
    #[serde(default)]
    pub pending_domain: Option<PendingDomain>,
    pub enabled: bool,
    #[serde(default)]
    pub needs_review: bool,
    #[serde(default)]
    pub credentials_need_review: bool,
    pub interval_seconds: u64,
    pub last_run_at: Option<i64>,
    #[serde(default)]
    pub last_run_status: Option<String>,
    pub next_run_at: Option<i64>,
    pub failures: u32,
    pub last_error: Option<CommandError>,
    #[serde(default)]
    pub running: bool,
}
pub struct PreparedPluginUpdate {
    pub token: String,
    pub previous_version: String,
    pub package: PluginPackage,
    pub manifest: Vec<u8>,
    pub component: Vec<u8>,
}
#[derive(Serialize)]
pub struct PluginUpdatePreview {
    pub token: String,
    pub current_version: String,
    pub package: PluginPackage,
    pub requires_review: bool,
}
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PluginPackageSelection {
    Installed {
        package: PluginPackage,
    },
    Update {
        current_package: PluginPackage,
        preview: Box<PluginUpdatePreview>,
    },
}
#[derive(Clone, Deserialize)]
pub struct InstanceInput {
    pub id: Option<String>,
    pub plugin_id: String,
    pub name: String,
    pub board_id: i64,
    pub config: BTreeMap<String, serde_json::Value>,
    pub allowed_domains: Vec<String>,
    pub enabled: bool,
    pub interval_seconds: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunLog {
    pub level: String,
    /// Allowlisted diagnostic code; absent for free-text messages (API 1.1.0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    pub count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PluginRun {
    pub id: String,
    pub instance_id: String,
    pub trigger: String,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub status: String,
    pub error: Option<CommandError>,
    pub logs: Vec<RunLog>,
    pub created: usize,
    pub updated: usize,
}
#[derive(Serialize)]
pub struct PluginState {
    pub packages: Vec<PluginPackage>,
    pub instances: Vec<PluginInstance>,
    pub safe_mode: bool,
}
pub fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
pub fn id() -> String {
    iroh::SecretKey::generate().public().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setting_descriptions_are_optional_and_preserve_localized_text() {
        let mut value = serde_json::json!({"key":"url","label":"URL","type":"text"});
        let field: SettingField = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(field.description, LocalizedText::Empty));
        for description in [
            serde_json::Value::Null,
            serde_json::json!("API base URL"),
            serde_json::json!({"en":"API base URL","zh-TW":"API 基底網址"}),
        ] {
            value["description"] = description.clone();
            let field: SettingField = serde_json::from_value(value.clone()).unwrap();
            assert_eq!(
                serde_json::to_value(field).unwrap()["description"],
                description
            );
        }
        value["description"] = serde_json::json!({"en":42});
        assert!(serde_json::from_value::<SettingField>(value).is_err());
    }
}
