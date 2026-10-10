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
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PluginPackage {
    pub id: String,
    pub name: LocalizedText,
    pub version: String,
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
    #[serde(default)]
    pub settings: Vec<SettingField>,
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
    pub code: String,
    pub count: u32,
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
