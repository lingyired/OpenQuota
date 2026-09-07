// Maps English strings produced by the Rust backend to i18n message keys.
// Exact-match entries cover fixed strings (metric labels, source notes,
// warnings, notices); pattern entries cover parameterized strings (e.g.
// "Retrying in about 5 minutes"). `tBackend(raw)` consults this glossary and
// falls back to the raw string when nothing matches, so unknown backend copy
// stays visible without crashing.
import type { Messages } from './messages/en';

type MessagePath<T = Messages> = {
  [K in keyof T & string]: T[K] extends string
    ? K
    : T[K] extends Record<string, unknown>
      ? `${K}.${MessagePath<T[K]>}`
      : never;
}[keyof T & string];

export type BackendGlossaryEntry = MessagePath | [MessagePath, Record<string, string | number>];

export const backendGlossary: Record<string, BackendGlossaryEntry> = {
  // Metric labels
  'Usage Today': 'metric.usageToday',
  'Usage Yesterday': 'metric.usageYesterday',
  'Last 30 Days': 'metric.last30Days',
  Session: 'metric.session',
  Weekly: 'metric.weekly',
  Monthly: 'metric.monthly',
  Credits: 'metric.credits',
  'Web Searches': 'metric.webSearches',
  'Extra Usage': 'metric.extraUsage',
  Usage: 'metric.usage',
  Daily: 'metric.daily',
  Requests: 'metric.requests',
  Status: 'metric.status',
  'Rate Limit Resets': 'metric.rateLimitResets',
  'Pay as you go': 'metric.payAsYouGo',
  'Extra Usage Balance': 'metric.extraUsageBalance',
  Spark: 'metric.spark',
  'Spark Weekly': 'metric.sparkWeekly',
  Disabled: 'metric.disabled',
  // Notices
  'Live usage paused': 'metric.liveUsagePaused',
  'Ready to retry': 'metric.readyToRetry',
  // Source notes
  'From your Claude usage history (estimated)': 'metric.sourceFromClaudeHistory',
  'From your Claude usage history and pi (estimated)': 'metric.sourceFromClaudeHistoryAndPi',
  'From your Codex logs (estimated)': 'metric.sourceFromCodexLogs',
  'From your Cursor usage export': 'metric.sourceFromCursorExport',
  'From your Grok logs (estimated)': 'metric.sourceFromGrokLogs',
  'From your OpenCode local database; missing costs use catalog estimates':
    'metric.sourceFromOpenCodeDb',
  // Warnings
  'Some OpenCode databases could not be read; available local usage is shown.':
    'metric.warnOpenCodeDbUnreadable',
  'OpenCode Go login data could not be read; local database usage is still shown.':
    'metric.warnOpenCodeGoLoginUnreadable',
  'OpenCode Go subscription required. Local usage is still shown while OpenCode Go quota data is unavailable.':
    'metric.warnOpenCodeGoSubscriptionRequired',
  'The refreshed Grok login is active for this session but could not be saved.': [
    'metric.warnRefreshLoginNotSaved',
    { provider: 'Grok' },
  ],
  'The refreshed Codex login is active for this session but could not be saved.': [
    'metric.warnRefreshLoginNotSaved',
    { provider: 'Codex' },
  ],
  'The refreshed Claude login is active for this session but could not be saved.': [
    'metric.warnRefreshLoginNotSaved',
    { provider: 'Claude' },
  ],
  'Re-login for live usage. Run `claude` and sign in again to restore subscription limits.':
    'metric.warnClaudeRelogin',
  'Claude live usage is rate limited; showing the last successful limits.':
    'metric.warnClaudeRateLimitedShowingStale',
  // Settings command errors surfaced through settingsController
  'Notification permission could not be requested.': 'app.errors.notificationPermission',
  'OpenQuota settings are temporarily unavailable.': 'settings.temporarilyUnavailable',
  'Settings changed before they could be saved. Please try again.': 'settings.changedBeforeSaved',
  'OpenQuota account settings could not be loaded.': 'settings.accountCouldNotLoad',
  'OpenQuota account names are temporarily unavailable.': 'settings.accountNamesUnavailable',
  'OpenQuota account names could not be saved.': 'settings.accountNamesCouldNotSave',
  'OpenQuota settings could not be saved.': 'settings.couldNotSave',
  'Unknown provider.': 'settings.unknownProvider',
  'Provider settings are unavailable.': 'settings.providerUnavailable',
  'Settings could not be saved.': 'settings.genericSaveError',
  'Settings could not be saved or reloaded.': 'settings.saveOrReloadError',
};

export interface BackendPattern {
  pattern: RegExp;
  key: MessagePath;
  params: string[];
}

export const backendPatterns: BackendPattern[] = [
  { pattern: /^(\S+) cap$/, key: 'metric.cap', params: ['value'] },
  {
    pattern: /^Retrying in about (\d+) minutes?$/,
    key: 'metric.retryingInAbout',
    params: ['count'],
  },
  {
    pattern: /^Showing the last successful limits · (.*)$/,
    key: 'metric.showingLastSuccessfulLimits',
    params: ['retry'],
  },
  {
    pattern: /^Claude live usage is rate limited; retrying in about (\d+) minutes?\.$/,
    key: 'metric.claudeRateLimitedRetrying',
    params: ['count'],
  },
  {
    pattern:
      /^Local (.+) usage history could not be refreshed; cached history is shown when available\.$/,
    key: 'metric.localHistoryRefreshFailed',
    params: ['provider'],
  },
  {
    pattern: /^From your (.+) usage history$/,
    key: 'metric.localUsageNote',
    params: ['provider'],
  },
  // Pacing / deadline strings produced by pacing.ts (frontend, rendered via
  // tBackend at the call site)
  { pattern: /^Resets soon$/, key: 'time.resetsSoon', params: [] },
  { pattern: /^Limit soon$/, key: 'time.limitSoon', params: [] },
  { pattern: /^Resets in (.+)$/, key: 'time.resetsIn', params: ['duration'] },
  { pattern: /^Limit in (.+)$/, key: 'time.limitIn', params: ['duration'] },
  { pattern: /^today at (.+)$/, key: 'time.todayAt', params: ['time'] },
  { pattern: /^tomorrow at (.+)$/, key: 'time.tomorrowAt', params: ['time'] },
  { pattern: /^(.+) at (.+)$/, key: 'time.onDateAt', params: ['date', 'time'] },
  { pattern: /^~(\d+)% spare$/, key: 'time.spare', params: ['value'] },
  { pattern: /^~(\d+)% left at reset$/, key: 'time.percentLeftAtReset', params: ['value'] },
  { pattern: /^~(\d+)% used at reset$/, key: 'time.percentUsedAtReset', params: ['value'] },
  {
    pattern: /^~(\d+)% over limit at reset$/,
    key: 'time.percentOverLimit',
    params: ['value'],
  },
];
