use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ApiKeyStatus {
    NotSet,
    FromEnvironment,
    FromConfig,
    /// A key Quota01 found in another app's own credential file, such as the credential the
    /// opencode CLI stores for OpenCode Go. The app key overrides it, and removing the app key
    /// falls back to it.
    FromCliSignIn,
    Saved,
    OverrideActive,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderApiKeyState {
    pub provider_id: String,
    pub status: ApiKeyStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ApiKeyMutationOutcome {
    #[serde(flatten)]
    pub state: ProviderApiKeyState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

/// Health of the shared credential vault.
///
/// `unrecoverable` means the vault file survived without its encryption key, so
/// every provider's save/read fails until the vault is reset. It is reported
/// separately from provider state because it is not any one provider's fault.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CredentialVaultState {
    pub unrecoverable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QuotaWindow {
    pub id: String,
    pub label: String,
    pub used_percent: f64,
    pub resets_at: Option<DateTime<Utc>>,
    pub period_seconds: u64,
    #[serde(default)]
    pub format: QuotaFormat,
    #[serde(default)]
    pub used_value: Option<f64>,
    #[serde(default)]
    pub limit_value: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(default)]
    pub estimated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_note: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MetricValueKind {
    Count,
    Dollars,
    Currency,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MetricValue {
    pub number: f64,
    pub kind: MetricValueKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default)]
    pub estimated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ValueMetric {
    pub id: String,
    pub label: String,
    pub values: Vec<MetricValue>,
    #[serde(default)]
    pub expiries_at: Vec<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum StatusTone {
    #[default]
    Neutral,
    Positive,
    Warning,
    Danger,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StatusMetric {
    pub id: String,
    pub label: String,
    pub text: String,
    #[serde(default)]
    pub tone: StatusTone,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ProviderNoticeTone {
    Info,
    Warning,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderNotice {
    pub id: String,
    pub title: String,
    pub message: String,
    pub tone: ProviderNoticeTone,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum QuotaFormat {
    #[default]
    Percent,
    Dollars,
    Count,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum UsageUnit {
    #[default]
    Tokens,
    Credits,
}

impl UsageUnit {
    pub fn is_tokens(value: &Self) -> bool {
        matches!(value, Self::Tokens)
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum UsageCompleteness {
    #[default]
    Unavailable,
    Complete,
    Partial,
}

fn default_cached_usage_completeness() -> UsageCompleteness {
    // Before completeness was persisted, every successfully cached usage history was complete.
    // Keep that meaning when loading snapshots written by older versions.
    UsageCompleteness::Complete
}

impl UsageCompleteness {
    pub fn is_complete(value: &Self) -> bool {
        matches!(value, Self::Complete)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UsagePeriod {
    pub tokens: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount: Option<f64>,
    #[serde(default, skip_serializing_if = "UsageUnit::is_tokens")]
    pub unit: UsageUnit,
    pub estimated_cost_usd: Option<f64>,
    #[serde(default = "default_true")]
    pub cost_estimated: bool,
    pub estimate_complete: bool,
    #[serde(default)]
    pub model_breakdown: Option<ModelUsageBreakdown>,
    #[serde(default)]
    pub unknown_models: Vec<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelUsageEntry {
    pub model: String,
    pub total_tokens: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount: Option<f64>,
    pub cost_usd: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variants: Option<Vec<ModelUsageVariant>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelUsageVariant {
    pub model: String,
    pub total_tokens: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount: Option<f64>,
    pub cost_usd: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelUsageBreakdown {
    pub models: Vec<ModelUsageEntry>,
    pub source_note: String,
    #[serde(default, skip_serializing_if = "UsageUnit::is_tokens")]
    pub unit: UsageUnit,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DailyUsage {
    pub date: String,
    pub tokens: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount: Option<f64>,
    #[serde(default, skip_serializing_if = "UsageUnit::is_tokens")]
    pub unit: UsageUnit,
    pub estimated_cost_usd: Option<f64>,
    pub estimate_complete: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UsageHistory {
    pub today: Option<UsagePeriod>,
    pub yesterday: Option<UsagePeriod>,
    pub last_30_days: Option<UsagePeriod>,
    pub daily: Vec<DailyUsage>,
    pub unknown_models: Vec<String>,
    #[serde(
        default = "default_cached_usage_completeness",
        skip_serializing_if = "UsageCompleteness::is_complete"
    )]
    pub completeness: UsageCompleteness,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CreditPackage {
    pub code: String,
    pub name: String,
    pub total: f64,
    pub remaining: f64,
    pub used: f64,
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub unlimited: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSnapshot {
    pub provider_id: String,
    pub plan: Option<String>,
    pub quotas: Vec<QuotaWindow>,
    #[serde(default)]
    pub credit_packages: Vec<CreditPackage>,
    #[serde(default)]
    pub value_metrics: Vec<ValueMetric>,
    #[serde(default)]
    pub status_metrics: Vec<StatusMetric>,
    #[serde(default)]
    pub notices: Vec<ProviderNotice>,
    pub usage: UsageHistory,
    pub warnings: Vec<String>,
    pub refreshed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SnapshotSource {
    None,
    Cache,
    Live,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ProviderErrorKind {
    Authentication,
    CredentialsUnavailable,
    LocalServiceUnavailable,
    Unsupported,
    Permission,
    RateLimited,
    Network,
    InvalidResponse,
    CredentialStorage,
    LocalData,
    Storage,
    Internal,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderViewState {
    pub snapshot: Option<ProviderSnapshot>,
    pub source: SnapshotSource,
    pub refreshing: bool,
    pub stale: bool,
    pub error: Option<String>,
    pub error_kind: Option<ProviderErrorKind>,
    pub last_attempt_at: Option<DateTime<Utc>>,
    /// 这个 provider 现在是不是停在失败状态。
    ///
    /// 界面顶部那一行只描述**当前正在看的** provider，所以这个判定要跟着每个
    /// provider 单独带上，而不是只留一个全局结论。判定与 `UsageViewState` 上那个
    /// 全局字段共用同一条规则：`error` 有值，或者上一次尝试失败后还没成功过。
    #[serde(default)]
    pub last_refresh_failed: bool,
}

impl Default for ProviderViewState {
    fn default() -> Self {
        Self {
            snapshot: None,
            source: SnapshotSource::None,
            refreshing: false,
            stale: false,
            error: None,
            error_kind: None,
            last_attempt_at: None,
            last_refresh_failed: false,
        }
    }
}

impl ProviderViewState {
    pub fn from_cache(snapshot: ProviderSnapshot) -> Self {
        Self {
            snapshot: Some(snapshot),
            source: SnapshotSource::Cache,
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MetricSection {
    AlwaysVisible,
    OnDemand,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum MetricSource {
    Quota {
        #[serde(rename = "sourceId")]
        source_id: String,
        #[serde(rename = "sessionWindow")]
        session_window: bool,
    },
    QuotaOrValue {
        #[serde(rename = "sourceId")]
        source_id: String,
        #[serde(rename = "sessionWindow")]
        session_window: bool,
    },
    Value {
        #[serde(rename = "sourceId")]
        source_id: String,
    },
    Status {
        #[serde(rename = "sourceId")]
        source_id: String,
    },
    CreditPackages,
    NearestCreditPackage {
        #[serde(rename = "sourceId")]
        source_id: String,
    },
    Usage {
        period: UsagePeriodSelection,
    },
    Trend,
}

impl MetricSource {
    pub fn source_id(&self) -> Option<&str> {
        match self {
            Self::Quota { source_id, .. }
            | Self::QuotaOrValue { source_id, .. }
            | Self::Value { source_id }
            | Self::Status { source_id }
            | Self::NearestCreditPackage { source_id } => Some(source_id),
            Self::CreditPackages | Self::Usage { .. } | Self::Trend => None,
        }
    }

    #[cfg(test)]
    pub fn session_window(&self) -> bool {
        matches!(
            self,
            Self::Quota {
                session_window: true,
                ..
            } | Self::QuotaOrValue {
                session_window: true,
                ..
            }
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TrayMetricDefinition {
    pub short_label: String,
    pub suffix: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MetricDefinition {
    pub id: String,
    pub label: String,
    pub source: MetricSource,
    pub pinnable: bool,
    pub default_enabled: bool,
    pub default_section: MetricSection,
    pub default_pinned: bool,
    /// Set when the provider reports quota percentages as floored whole numbers, so a reported
    /// `0` means "below 1%" rather than "no usage".
    #[serde(default)]
    pub floored_percent: bool,
    pub tray: Option<TrayMetricDefinition>,
}

impl MetricDefinition {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        source: MetricSource,
        pinnable: bool,
        default_enabled: bool,
        default_section: MetricSection,
        default_pinned: bool,
        tray_short_label: Option<&str>,
        tray_suffix: Option<&str>,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            source,
            pinnable,
            default_enabled,
            default_section,
            default_pinned,
            floored_percent: false,
            tray: tray_short_label.map(|short_label| TrayMetricDefinition {
                short_label: short_label.into(),
                suffix: tray_suffix.map(str::to_owned),
            }),
        }
    }

    /// Marks the metric's percentage as floored by the provider so the UI can
    /// render sub-1% usage instead of a misleading `0%`.
    pub fn with_floored_percent(mut self) -> Self {
        self.floored_percent = true;
        self
    }

    #[allow(clippy::too_many_arguments)]
    pub fn quota(
        id: &str,
        label: &str,
        source_id: &str,
        session_window: bool,
        default_enabled: bool,
        default_section: MetricSection,
        default_pinned: bool,
        tray_short_label: &str,
    ) -> Self {
        Self::new(
            id,
            label,
            MetricSource::Quota {
                source_id: source_id.into(),
                session_window,
            },
            true,
            default_enabled,
            default_section,
            default_pinned,
            Some(tray_short_label),
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn quota_or_value(
        id: &str,
        label: &str,
        source_id: &str,
        default_enabled: bool,
        default_section: MetricSection,
        default_pinned: bool,
        tray_short_label: &str,
    ) -> Self {
        Self::new(
            id,
            label,
            MetricSource::QuotaOrValue {
                source_id: source_id.into(),
                session_window: false,
            },
            true,
            default_enabled,
            default_section,
            default_pinned,
            Some(tray_short_label),
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn value(
        id: &str,
        label: &str,
        source_id: &str,
        default_enabled: bool,
        default_section: MetricSection,
        default_pinned: bool,
        tray_short_label: &str,
        tray_suffix: Option<&str>,
    ) -> Self {
        Self::new(
            id,
            label,
            MetricSource::Value {
                source_id: source_id.into(),
            },
            true,
            default_enabled,
            default_section,
            default_pinned,
            Some(tray_short_label),
            tray_suffix,
        )
    }

    #[allow(clippy::too_many_arguments, dead_code)]
    pub fn status(
        id: &str,
        label: &str,
        source_id: &str,
        default_enabled: bool,
        default_section: MetricSection,
        default_pinned: bool,
        tray_short_label: &str,
    ) -> Self {
        Self::new(
            id,
            label,
            MetricSource::Status {
                source_id: source_id.into(),
            },
            true,
            default_enabled,
            default_section,
            default_pinned,
            Some(tray_short_label),
            None,
        )
    }

    pub fn usage(
        id: &str,
        label: &str,
        period: UsagePeriodSelection,
        default_section: MetricSection,
        tray_short_label: &str,
    ) -> Self {
        Self::new(
            id,
            label,
            MetricSource::Usage { period },
            true,
            true,
            default_section,
            false,
            Some(tray_short_label),
            None,
        )
    }

    pub fn trend(id: &str) -> Self {
        Self::new(
            id,
            "Usage Trend",
            MetricSource::Trend,
            false,
            true,
            MetricSection::AlwaysVisible,
            false,
            None,
            None,
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderLink {
    pub label: String,
    pub url: String,
}

impl ProviderLink {
    pub fn new(label: &str, url: &str) -> Self {
        Self {
            label: label.into(),
            url: url.into(),
        }
    }

    pub fn visible(&self) -> Option<Self> {
        let label = self.label.trim();
        let url = self.url.trim();
        if label.is_empty()
            || url.is_empty()
            || !(url.starts_with("https://") || url.starts_with("http://"))
        {
            return None;
        }
        Some(Self::new(label, url))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDefinition {
    pub id: String,
    pub display_name: String,
    pub short_name: String,
    pub fallback_enabled: bool,
    pub local_usage_source_note: Option<String>,
    #[serde(default)]
    pub links: Vec<ProviderLink>,
    pub metrics: Vec<MetricDefinition>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCatalog {
    pub providers: Vec<ProviderDefinition>,
    #[serde(default)]
    pub api_key_provider_ids: Vec<String>,
    #[serde(default)]
    pub webview_auth_provider_ids: Vec<String>,
    #[serde(default)]
    pub device_code_sign_in_provider_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MetricLayout {
    pub id: String,
    pub enabled: bool,
    pub section: MetricSection,
    pub pinned: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderLayout {
    pub id: String,
    pub enabled: bool,
    pub detected: bool,
    pub expanded: bool,
    #[serde(default)]
    pub use_proxy: bool,
    #[cfg(not(target_os = "macos"))]
    #[serde(default)]
    pub keychain_access_granted: bool,
    pub metrics: Vec<MetricLayout>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ThemePreference {
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DensityPreference {
    Default,
    Compact,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MenuBarStyle {
    Text,
    Bars,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum UsageDisplay {
    Used,
    Left,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ResetDisplay {
    Countdown,
    Exact,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TimeFormatPreference {
    #[default]
    System,
    TwelveHour,
    TwentyFourHour,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[repr(u8)]
pub enum LogLevel {
    Error = 0,
    Warn = 1,
    Debug = 3,
    #[default]
    #[serde(other)]
    Info = 2,
}

impl LogLevel {
    pub fn from_severity(value: u8) -> Self {
        match value {
            0 => Self::Error,
            1 => Self::Warn,
            3 => Self::Debug,
            _ => Self::Info,
        }
    }

    pub fn log_label(self) -> &'static str {
        match self {
            Self::Error => "ERROR",
            Self::Warn => "WARN",
            Self::Info => "INFO",
            Self::Debug => "DEBUG",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TotalSpendMetric {
    Cost,
    CostPerMillion,
    Tokens,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum UsagePeriodSelection {
    Today,
    Yesterday,
    Last30Days,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum WindowMode {
    Floating,
    #[default]
    #[serde(other)]
    Popup,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum TaskbandSide {
    Left,
    #[default]
    Right,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum TaskbandColorStyle {
    Default,
    Solid { value: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct TaskbandLayout {
    pub enabled: bool,
    pub side: Option<TaskbandSide>,
    pub top_color: Option<TaskbandColorStyle>,
    pub bottom_color: Option<TaskbandColorStyle>,
    pub top_bold: bool,
    pub bottom_bold: bool,
    pub top_size: f64,
    pub bottom_size: f64,
    pub top_align: i32,
    pub bottom_align: i32,
    pub padding_left: i32,
    pub padding_right: i32,
}

impl Default for TaskbandLayout {
    fn default() -> Self {
        Self {
            enabled: true,
            side: None,
            top_color: None,
            bottom_color: None,
            top_bold: false,
            bottom_bold: false,
            top_size: 9.0,
            bottom_size: 9.0,
            top_align: 0,
            bottom_align: 0,
            padding_left: 4,
            padding_right: 4,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct TaskbandPreferences {
    pub enabled: bool,
    pub default_side: TaskbandSide,
    pub margin: i32,
    pub edge_margin_left: i32,
    pub edge_margin_right: i32,
}

impl Default for TaskbandPreferences {
    fn default() -> Self {
        Self {
            enabled: true,
            default_side: TaskbandSide::Right,
            margin: 4,
            edge_margin_left: 0,
            edge_margin_right: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LanguagePreference {
    #[default]
    #[serde(rename = "system", alias = "System")]
    System,
    #[serde(alias = "En")]
    En,
    #[serde(rename = "zh-CN", alias = "ZhCn")]
    ZhCn,
    #[serde(rename = "zh-TW", alias = "ZhTw")]
    ZhTw,
    #[serde(alias = "Es")]
    Es,
    #[serde(rename = "pt-BR", alias = "PtBr")]
    PtBr,
    #[serde(alias = "Ja")]
    Ja,
    #[serde(alias = "Ko")]
    Ko,
    #[serde(alias = "De")]
    De,
    #[serde(alias = "Fr")]
    Fr,
    #[serde(alias = "Ru")]
    Ru,
    #[serde(alias = "Hi")]
    Hi,
    #[serde(alias = "Ar")]
    Ar,
    #[serde(alias = "It")]
    It,
    #[serde(alias = "Pl")]
    Pl,
    #[serde(alias = "Tr")]
    Tr,
    #[serde(alias = "Vi")]
    Vi,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct NotificationPreferences {
    pub almost_out: bool,
    pub cutting_it_close: bool,
    pub will_run_out: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    pub schema_version: u32,
    pub providers: Vec<ProviderLayout>,
    pub known_provider_ids: Vec<String>,
    pub provider_names: BTreeMap<String, String>,
    pub language: LanguagePreference,
    pub show_total_spend: bool,
    pub theme: ThemePreference,
    pub density: DensityPreference,
    pub reduce_animations: bool,
    pub window_mode: WindowMode,
    pub menu_bar_style: MenuBarStyle,
    pub usage_display: UsageDisplay,
    pub reset_display: ResetDisplay,
    pub time_format: TimeFormatPreference,
    pub always_show_pacing: bool,
    pub launch_at_login: bool,
    pub auto_check_updates: bool,
    pub dismissed_update_version: Option<String>,
    pub last_update_check_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy_url: Option<String>,
    pub global_shortcut: Option<String>,
    pub log_level: LogLevel,
    pub notifications: NotificationPreferences,
    pub total_spend_metric: TotalSpendMetric,
    pub total_spend_period: UsagePeriodSelection,
    pub detection_notice_dismissed: bool,
    #[serde(default)]
    pub taskband: TaskbandPreferences,
    #[serde(default)]
    pub taskband_providers: BTreeMap<String, TaskbandLayout>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            schema_version: 10,
            providers: Vec::new(),
            known_provider_ids: Vec::new(),
            provider_names: BTreeMap::new(),
            language: LanguagePreference::System,
            // The dashboard's cost surface is off unless a user opted in while the
            // control was still offered. This decides the value for a fresh install
            // only: an existing record keeps whatever it persisted, so nobody's
            // dashboard changes under them on upgrade.
            show_total_spend: false,
            theme: ThemePreference::System,
            density: DensityPreference::Default,
            reduce_animations: false,
            window_mode: WindowMode::Popup,
            menu_bar_style: MenuBarStyle::Text,
            usage_display: UsageDisplay::Left,
            reset_display: ResetDisplay::Countdown,
            time_format: TimeFormatPreference::System,
            always_show_pacing: true,
            launch_at_login: false,
            auto_check_updates: true,
            dismissed_update_version: None,
            last_update_check_at: None,
            proxy_url: None,
            global_shortcut: None,
            log_level: LogLevel::Info,
            notifications: NotificationPreferences::default(),
            total_spend_metric: TotalSpendMetric::Cost,
            total_spend_period: UsagePeriodSelection::Today,
            detection_notice_dismissed: false,
            taskband: TaskbandPreferences::default(),
            taskband_providers: BTreeMap::new(),
        }
    }
}

impl AppSettings {
    pub fn provider_display_name<'a>(&'a self, definition: &'a ProviderDefinition) -> &'a str {
        self.provider_names
            .get(&definition.id)
            .map(String::as_str)
            .unwrap_or(&definition.display_name)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SettingsViewState {
    pub settings: AppSettings,
    pub settings_revision: u64,
    pub account_revision: u64,
    pub renamable_provider_ids: Vec<String>,
    pub notification_permission: String,
    pub integration_error: Option<String>,
    pub tray_available: bool,
    pub platform_summary: Option<String>,
    pub provider_instance_count: usize,
    pub provider_instance_failures: Vec<String>,
}

/// 设备码登录的开始结果：只带前端展示所必需的信息。
///
/// `login_id` 是 Quota01 自己生成的句柄，与服务器下发的 `state` 分开保存，
/// 前端因此永远拿不到可以直接换 token 的 `state`。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCodeChallenge {
    pub login_id: String,
    pub verification_uri: String,
    pub expires_in: u64,
}

/// 设备码登录的轮询结果，会原样发给前端。
///
/// 这里刻意不放会话：token 只在 Rust 内部流转，绝不经过前端。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCodePoll {
    pub done: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{
        ApiKeyMutationOutcome, ApiKeyStatus, AppSettings, LanguagePreference, LogLevel,
        MetricValueKind, ProviderApiKeyState, ProviderErrorKind, ProviderLink, ProviderSnapshot,
        ProviderViewState, UsageCompleteness, UsagePeriod, WindowMode,
    };

    /// The dashboard's cost surface is hidden and off for a fresh install. This
    /// only pins the default; a stored record keeps whatever it persisted.
    #[test]
    fn total_spend_is_off_by_default() {
        assert!(!AppSettings::default().show_total_spend);
    }

    /// A record written while the toggle was still offered keeps its own value:
    /// the default must not reach in and switch a user's dashboard off.
    #[test]
    fn a_persisted_total_spend_choice_survives_deserialization() {
        let stored = serde_json::json!({ "showTotalSpend": true });
        let restored: AppSettings = serde_json::from_value(stored).expect("stored settings load");
        assert!(
            restored.show_total_spend,
            "an opted-in record must not be reset by the new default"
        );

        // A record that predates the field falls back to the default (off).
        let legacy: AppSettings =
            serde_json::from_value(serde_json::json!({})).expect("legacy settings load");
        assert!(!legacy.show_total_spend);
    }

    #[test]
    fn language_preference_uses_frontend_contract_and_reads_legacy_rust_names() {
        let cases = [
            (LanguagePreference::System, "system", "System"),
            (LanguagePreference::En, "en", "En"),
            (LanguagePreference::ZhCn, "zh-CN", "ZhCn"),
            (LanguagePreference::ZhTw, "zh-TW", "ZhTw"),
            (LanguagePreference::Es, "es", "Es"),
            (LanguagePreference::PtBr, "pt-BR", "PtBr"),
            (LanguagePreference::Ja, "ja", "Ja"),
            (LanguagePreference::Ko, "ko", "Ko"),
            (LanguagePreference::De, "de", "De"),
            (LanguagePreference::Fr, "fr", "Fr"),
            (LanguagePreference::Ru, "ru", "Ru"),
            (LanguagePreference::Hi, "hi", "Hi"),
            (LanguagePreference::Ar, "ar", "Ar"),
            (LanguagePreference::It, "it", "It"),
            (LanguagePreference::Pl, "pl", "Pl"),
            (LanguagePreference::Tr, "tr", "Tr"),
            (LanguagePreference::Vi, "vi", "Vi"),
        ];

        for (preference, frontend_value, legacy_value) in cases {
            assert_eq!(
                serde_json::to_value(preference).unwrap(),
                serde_json::json!(frontend_value)
            );
            assert_eq!(
                serde_json::from_value::<LanguagePreference>(serde_json::json!(frontend_value))
                    .unwrap(),
                preference
            );
            assert_eq!(
                serde_json::from_value::<LanguagePreference>(serde_json::json!(legacy_value))
                    .unwrap(),
                preference
            );
        }
    }

    #[test]
    fn currency_metric_kind_uses_the_frontend_contract_name() {
        assert_eq!(
            serde_json::to_value(MetricValueKind::Currency).unwrap(),
            serde_json::json!("currency")
        );
    }

    #[test]
    fn older_settings_default_new_update_state_fields() {
        let mut value = serde_json::to_value(AppSettings::default()).unwrap();
        let object = value.as_object_mut().unwrap();
        object.remove("dismissedUpdateVersion");
        object.remove("lastUpdateCheckAt");
        object.remove("logLevel");
        object.remove("providerNames");
        object.remove("windowMode");
        object.remove("reduceAnimations");

        let settings: AppSettings = serde_json::from_value(value).unwrap();
        assert_eq!(settings.dismissed_update_version, None);
        assert!(settings.provider_names.is_empty());
        assert_eq!(settings.last_update_check_at, None);
        assert_eq!(settings.log_level, LogLevel::Info);
        assert_eq!(settings.window_mode, WindowMode::Popup);
        assert!(!settings.reduce_animations);
    }

    #[test]
    fn language_preferences_read_legacy_and_current_disk_values() {
        assert_eq!(
            serde_json::from_str::<LanguagePreference>(r#""system""#).unwrap(),
            LanguagePreference::System
        );
        assert_eq!(
            serde_json::from_str::<LanguagePreference>(r#""System""#).unwrap(),
            LanguagePreference::System
        );
        assert_eq!(
            serde_json::to_string(&LanguagePreference::System).unwrap(),
            r#""system""#
        );
    }

    #[test]
    fn always_show_pacing_defaults_to_enabled() {
        assert!(AppSettings::default().always_show_pacing);
    }

    #[test]
    fn legacy_app_menubar_setting_is_ignored() {
        let mut value = serde_json::to_value(AppSettings::default()).unwrap();
        value["showAppMenubar"] = serde_json::json!(false);
        let settings: AppSettings = serde_json::from_value(value).unwrap();
        assert_eq!(
            serde_json::to_value(settings).unwrap()["showAppMenubar"],
            serde_json::Value::Null
        );
    }

    #[test]
    fn unknown_persisted_log_levels_fall_back_to_info() {
        let mut value = serde_json::to_value(AppSettings::default()).unwrap();
        value["logLevel"] = serde_json::json!("trace");
        let settings: AppSettings = serde_json::from_value(value).unwrap();
        assert_eq!(settings.log_level, LogLevel::Info);
    }

    #[test]
    fn unknown_persisted_window_modes_fall_back_to_popup() {
        let mut value = serde_json::to_value(AppSettings::default()).unwrap();
        value["windowMode"] = serde_json::json!("detached");
        let settings: AppSettings = serde_json::from_value(value).unwrap();
        assert_eq!(settings.window_mode, WindowMode::Popup);
    }

    #[test]
    fn provider_error_kind_uses_the_frontend_contract_name() {
        let state = ProviderViewState {
            error: Some("Could not connect to the provider.".into()),
            error_kind: Some(ProviderErrorKind::Network),
            ..ProviderViewState::default()
        };

        let value = serde_json::to_value(state).unwrap();
        assert_eq!(value["errorKind"], "network");
    }

    #[test]
    fn api_key_state_exposes_status_without_a_secret_field() {
        let value = serde_json::to_value(ProviderApiKeyState {
            provider_id: "openrouter".into(),
            status: ApiKeyStatus::OverrideActive,
        })
        .unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "providerId": "openrouter",
                "status": "overrideActive"
            })
        );
    }

    #[test]
    fn applied_api_key_mutations_only_add_a_warning_when_reconciliation_is_incomplete() {
        let value = serde_json::to_value(ApiKeyMutationOutcome {
            state: ProviderApiKeyState {
                provider_id: "openrouter".into(),
                status: ApiKeyStatus::Saved,
            },
            warning: Some("Provider status could not be refreshed.".into()),
        })
        .unwrap();

        assert_eq!(
            value,
            serde_json::json!({
                "providerId": "openrouter",
                "status": "saved",
                "warning": "Provider status could not be refreshed."
            })
        );
    }

    #[test]
    fn cached_usage_periods_default_to_local_cost_estimates() {
        let period: UsagePeriod = serde_json::from_str(
            r#"{"tokens":42,"estimatedCostUsd":0.12,"estimateComplete":true,"unknownModels":[]}"#,
        )
        .unwrap();
        assert!(period.cost_estimated);
    }

    #[test]
    fn cached_snapshots_default_new_dynamic_rows() {
        let snapshot: ProviderSnapshot = serde_json::from_str(
            r#"{
                "providerId":"codex",
                "plan":null,
                "quotas":[{
                    "id":"session",
                    "label":"Session",
                    "usedPercent":10,
                    "resetsAt":null,
                    "periodSeconds":18000,
                    "format":"percent",
                    "usedValue":null,
                    "limitValue":null
                }],
                "valueMetrics":[{
                    "id":"credits",
                    "label":"Credits",
                    "values":[{"number":2,"kind":"count"}],
                    "expiriesAt":[]
                }],
                "usage":{"today":null,"yesterday":null,"last30Days":null,"daily":[],"unknownModels":[]},
                "warnings":[],
                "refreshedAt":"2026-07-15T00:00:00Z"
            }"#,
        )
        .unwrap();
        assert_eq!(snapshot.quotas[0].unit, None);
        assert!(!snapshot.quotas[0].estimated);
        assert_eq!(snapshot.quotas[0].source_note, None);
        assert!(!snapshot.value_metrics[0].values[0].estimated);
        assert!(snapshot.status_metrics.is_empty());
        assert!(snapshot.notices.is_empty());
        assert_eq!(snapshot.usage.completeness, UsageCompleteness::Complete);
    }

    #[test]
    fn provider_link_visibility_matches_the_trimmed_http_contract() {
        let links = [
            ProviderLink::new(" Status ", " https://status.example.com/ "),
            ProviderLink::new("HTTP", "http://example.com/dashboard"),
            ProviderLink::new("", "https://example.com/"),
            ProviderLink::new("No URL", " "),
            ProviderLink::new("FTP", "ftp://example.com/"),
            ProviderLink::new("JS", "javascript:alert(1)"),
            ProviderLink::new("Mail", "mailto:a@example.com"),
            ProviderLink::new("No scheme", "example.com"),
        ];

        assert_eq!(
            links
                .iter()
                .filter_map(ProviderLink::visible)
                .collect::<Vec<_>>(),
            [
                ProviderLink::new("Status", "https://status.example.com/"),
                ProviderLink::new("HTTP", "http://example.com/dashboard"),
            ]
        );
    }

    #[test]
    fn local_credential_error_kinds_use_camel_case_names() {
        let kinds = [
            (
                ProviderErrorKind::CredentialsUnavailable,
                "credentialsUnavailable",
            ),
            (
                ProviderErrorKind::LocalServiceUnavailable,
                "localServiceUnavailable",
            ),
            (ProviderErrorKind::Unsupported, "unsupported"),
        ];

        for (kind, expected) in kinds {
            assert_eq!(
                serde_json::to_string(&kind).unwrap(),
                format!("\"{expected}\"")
            );
        }
    }
}
