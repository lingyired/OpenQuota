export interface QuotaWindow {
  id: string;
  label: string;
  usedPercent: number;
  resetsAt: string | null;
  periodSeconds: number;
  format: 'percent' | 'dollars' | 'count';
  usedValue: number | null;
  limitValue: number | null;
  unit?: string | null;
  estimated: boolean;
  sourceNote?: string | null;
}

export interface MetricValue {
  number: number;
  kind: 'count' | 'dollars';
  label?: string | null;
  estimated: boolean;
}

export interface ValueMetric {
  id: string;
  label: string;
  values: MetricValue[];
  expiriesAt: string[];
}

export interface StatusMetric {
  id: string;
  label: string;
  text: string;
  tone: 'neutral' | 'positive' | 'warning' | 'danger';
  subtitle?: string | null;
}

export type ResetClaimOutcome = 'success' | 'nothingToReset' | 'noCredit' | 'failed';

export interface ProviderNotice {
  id: string;
  title: string;
  message: string;
  tone: 'info' | 'warning';
}

export type UsageUnit = 'tokens' | 'credits';
export type UsageCompleteness = 'unavailable' | 'complete' | 'partial';

export interface UsagePeriod {
  tokens: number;
  amount?: number | null;
  unit?: UsageUnit;
  estimatedCostUsd: number | null;
  costEstimated: boolean;
  estimateComplete: boolean;
  modelBreakdown?: ModelUsageBreakdown | null;
  unknownModels?: string[];
}

export interface ModelUsageEntry {
  model: string;
  totalTokens: number;
  amount?: number | null;
  costUsd: number | null;
  variants?: ModelUsageVariant[] | null;
}

export interface ModelUsageVariant {
  model: string;
  totalTokens: number;
  amount?: number | null;
  costUsd: number | null;
}

export interface ModelUsageBreakdown {
  models: ModelUsageEntry[];
  sourceNote: string;
  unit?: UsageUnit;
}

export interface DailyUsage {
  date: string;
  tokens: number;
  amount?: number | null;
  unit?: UsageUnit;
  estimatedCostUsd: number | null;
  estimateComplete: boolean;
}

export interface UsageHistory {
  today: UsagePeriod | null;
  yesterday: UsagePeriod | null;
  last30Days: UsagePeriod | null;
  daily: DailyUsage[];
  unknownModels: string[];
  completeness?: UsageCompleteness;
}

export interface CreditPackage {
  code: string;
  name: string;
  total: number;
  remaining: number;
  used: number;
  expiresAt: string | null;
  unlimited?: boolean;
}

export interface ProviderSnapshot {
  providerId: string;
  plan: string | null;
  quotas: QuotaWindow[];
  creditPackages: CreditPackage[];
  valueMetrics: ValueMetric[];
  statusMetrics: StatusMetric[];
  notices: ProviderNotice[];
  usage: UsageHistory;
  warnings: string[];
  refreshedAt: string;
}

export type ProviderErrorKind =
  | 'authentication'
  | 'permission'
  | 'rateLimited'
  | 'network'
  | 'invalidResponse'
  | 'credentialStorage'
  | 'localData'
  | 'storage'
  | 'internal';

export interface ProviderViewState {
  snapshot: ProviderSnapshot | null;
  source: 'none' | 'cache' | 'live';
  refreshing: boolean;
  stale: boolean;
  error: string | null;
  errorKind: ProviderErrorKind | null;
  lastAttemptAt: string | null;
}

export interface UsageViewState {
  providers: Record<string, ProviderViewState>;
  lastFullRefreshAt?: string | null;
}

export type MetricSection = 'alwaysVisible' | 'onDemand';

export type MetricSource =
  | { kind: 'quota'; sourceId: string; sessionWindow: boolean }
  | { kind: 'quotaOrValue'; sourceId: string; sessionWindow: boolean }
  | { kind: 'value'; sourceId: string }
  | { kind: 'status'; sourceId: string }
  | { kind: 'creditPackages' }
  | { kind: 'usage'; period: 'today' | 'yesterday' | 'last30Days' }
  | { kind: 'trend' };

export interface TrayMetricDefinition {
  shortLabel: string;
  suffix: string | null;
}

export interface MetricDefinition {
  id: string;
  label: string;
  source: MetricSource;
  pinnable: boolean;
  defaultEnabled: boolean;
  defaultSection: MetricSection;
  defaultPinned: boolean;
  flooredPercent?: boolean;
  tray: TrayMetricDefinition | null;
}

export interface ProviderLink {
  label: string;
  url: string;
}

export type ApiKeyStatus = 'notSet' | 'fromEnvironment' | 'fromConfig' | 'saved' | 'overrideActive';

export interface ProviderApiKeyState {
  providerId: string;
  status: ApiKeyStatus;
}

export interface ApiKeyMutationOutcome extends ProviderApiKeyState {
  warning?: string;
}

export interface ProviderDefinition {
  id: string;
  displayName: string;
  shortName: string;
  fallbackEnabled: boolean;
  localUsageSourceNote: string | null;
  links: ProviderLink[];
  metrics: MetricDefinition[];
}

export interface ProviderCatalog {
  providers: ProviderDefinition[];
  apiKeyProviderIds?: string[];
  webviewAuthProviderIds?: string[];
}

export interface MetricLayout {
  id: string;
  enabled: boolean;
  section: MetricSection;
  pinned: boolean;
}

export interface ProviderLayout {
  id: string;
  enabled: boolean;
  detected: boolean;
  expanded: boolean;
  metrics: MetricLayout[];
}

export type LanguagePreference =
  | 'system'
  | 'en'
  | 'zh-CN'
  | 'zh-TW'
  | 'es'
  | 'pt-BR'
  | 'ja'
  | 'ko'
  | 'de'
  | 'fr'
  | 'ru'
  | 'hi'
  | 'ar'
  | 'it'
  | 'pl'
  | 'tr'
  | 'vi';

export interface NotificationPreferences {
  almostOut: boolean;
  cuttingItClose: boolean;
  willRunOut: boolean;
}

export type TaskbandSide = 'left' | 'right';

export type TaskbandColorStyle = { type: 'default' } | { type: 'solid'; value: string };

export interface TaskbandLayout {
  enabled: boolean;
  side: TaskbandSide | null;
  topColor: TaskbandColorStyle | null;
  bottomColor: TaskbandColorStyle | null;
  topBold: boolean;
  bottomBold: boolean;
  topSize: number;
  bottomSize: number;
  topAlign: number;
  bottomAlign: number;
  paddingLeft: number;
  paddingRight: number;
}

export interface TaskbandPreferences {
  enabled: boolean;
  defaultSide: TaskbandSide;
  margin: number;
  edgeMarginLeft: number;
  edgeMarginRight: number;
}

export interface AppSettings {
  schemaVersion: number;
  providers: ProviderLayout[];
  knownProviderIds: string[];
  providerNames: Record<string, string>;
  language: LanguagePreference;
  showTotalSpend: boolean;
  theme: 'system' | 'light' | 'dark';
  density: 'default' | 'compact';
  reduceAnimations: boolean;
  windowMode: 'popup' | 'floating';
  menuBarStyle: 'text' | 'bars';
  usageDisplay: 'used' | 'left';
  resetDisplay: 'countdown' | 'exact';
  timeFormat: 'system' | 'twelveHour' | 'twentyFourHour';
  alwaysShowPacing: boolean;
  launchAtLogin: boolean;
  autoCheckUpdates: boolean;
  dismissedUpdateVersion: string | null;
  lastUpdateCheckAt: string | null;
  globalShortcut: string | null;
  logLevel: 'error' | 'warn' | 'info' | 'debug';
  notifications: NotificationPreferences;
  totalSpendMetric: 'cost' | 'costPerMillion' | 'tokens';
  totalSpendPeriod: 'today' | 'yesterday' | 'last30Days';
  detectionNoticeDismissed: boolean;
  taskband: TaskbandPreferences;
  taskbandProviders: Record<string, TaskbandLayout>;
}

export interface UpdateStatus {
  available: boolean;
  currentVersion: string;
  version: string | null;
  body: string | null;
  installable: boolean;
  releaseUrl: string;
}

export interface UpdateProgress {
  phase: 'downloading' | 'retrying' | 'installing';
  downloaded: number;
  total: number | null;
  percent: number | null;
}

export interface UpdateFailure {
  code: string;
  message: string;
  action: string;
  retryable: boolean;
}

export interface SettingsViewState {
  settings: AppSettings;
  settingsRevision: number;
  accountRevision: number;
  renamableProviderIds: string[];
  notificationPermission: 'granted' | 'denied' | 'prompt' | 'unavailable';
  integrationError: string | null;
  trayAvailable: boolean;
  platformSummary: string | null;
}

export interface BootstrapState {
  usage: UsageViewState;
  settings: SettingsViewState;
  catalog: ProviderCatalog;
}
