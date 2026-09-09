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
  'Usage Trend': 'metric.usageTrend',
  'Last 30 Days': 'metric.last30Days',
  Session: 'metric.session',
  'Session (5h)': 'metric.session5h',
  Weekly: 'metric.weekly',
  Monthly: 'metric.monthly',
  Credits: 'metric.credits',
  'Web Searches': 'metric.webSearches',
  'Extra Usage': 'metric.extraUsage',
  Usage: 'metric.usage',
  Daily: 'metric.daily',
  Requests: 'metric.requests',
  Status: 'metric.status',
  Dashboard: 'metric.dashboard',
  Activity: 'metric.activity',
  'API Keys': 'metric.apiKeys',
  'Rate Limit Resets': 'metric.rateLimitResets',
  Today: 'metric.today',
  Yesterday: 'metric.yesterday',
  'This Week': 'metric.thisWeek',
  'This Month': 'metric.thisMonth',
  Balance: 'metric.balance',
  'Extra Balance': 'metric.extraBalance',
  'Org Credits': 'metric.orgCredits',
  'Org Spend': 'metric.orgSpend',
  'Key Limit': 'metric.keyLimit',
  Chat: 'metric.chat',
  Completions: 'metric.completions',
  'API Usage': 'metric.apiUsage',
  'API usage': 'metric.apiUsage',
  'Auto Usage': 'metric.autoUsage',
  'Auto usage': 'metric.autoUsage',
  'On-demand': 'metric.onDemand',
  'Total Usage': 'metric.totalUsage',
  Claude: 'metric.claude',
  'Claude Weekly': 'metric.claudeWeekly',
  Fable: 'metric.fable',
  Sonnet: 'metric.sonnet',
  credits: 'metric.creditsLower',
  available: 'metric.available',
  spent: 'time.spent',
  resets: 'time.resets',
  'Pay as you go': 'metric.payAsYouGo',
  'Extra Usage Balance': 'metric.extraUsageBalance',
  Spark: 'metric.spark',
  'Spark Weekly': 'metric.sparkWeekly',
  requests: 'units.requests',
  searches: 'units.searches',
  Disabled: 'metric.disabled',
  Free: 'plan.free',
  Plus: 'plan.plus',
  Pro: 'plan.pro',
  'Pro 5x': 'plan.pro5x',
  'Pro 20x': 'plan.pro20x',
  Business: 'plan.business',
  Enterprise: 'plan.enterprise',
  Ultra: 'plan.ultra',
  'Token Plan': 'plan.tokenPlan',
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
  // Provider errors
  'Not logged in. Run `codex` to authenticate.': [
    'providerError.notLoggedInRunCmd',
    { cmd: 'codex' },
  ],
  'Not logged in. Run `claude` to authenticate.': [
    'providerError.notLoggedInRunCmd',
    { cmd: 'claude' },
  ],
  'Claude Desktop login found, but its macOS-only encrypted session cannot be reused safely. Run `claude` in a terminal and sign in once.':
    'providerError.claudeDesktopAppOnly',
  'Subscription usage is unavailable for API-key-only logins. Sign in to Codex with ChatGPT.':
    'providerError.codexApiKeyOnly',
  'Add a Z.ai API key in Customize, set ZAI_API_KEY, or configure ~/.config/openquota/zai.json.':
    'providerError.addZaiApiKey',
  'Add a Kimi API key in Customize or set KIMI_API_KEY.': 'providerError.addKimiApiKey',
  'Add a MiniMax API key in Customize or set MINIMAX_API_KEY.': 'providerError.addMiniMaxApiKey',
  'Add an OpenRouter API key in Customize to view usage.': 'providerError.addOpenRouterApiKey',
  'The Kimi API key is invalid. Check it in the Kimi Code console.':
    'providerError.apiKeyInvalidInConsole',
  'The MiniMax API key is invalid. Check it at minimax.io.': [
    'providerError.apiKeyInvalid',
    { provider: 'MiniMax', url: 'minimax.io' },
  ],
  'The OpenRouter API key is invalid. Check it at openrouter.ai/keys.': [
    'providerError.apiKeyInvalid',
    { provider: 'OpenRouter', url: 'openrouter.ai/keys' },
  ],
  'The Z.ai API key is invalid. Check it at z.ai/manage-apikey/apikey-list.': [
    'providerError.apiKeyInvalid',
    { provider: 'Z.ai', url: 'z.ai/manage-apikey/apikey-list' },
  ],
  'OpenCode local usage data is temporarily unavailable.':
    'providerError.openCodeLocalUsageUnavailable',
  'Usage request failed after refresh. Try again.': 'providerError.usageRequestFailedAfterRefresh',
  'Total usage limit missing from API response.': 'providerError.totalUsageLimitMissing',
  'OpenQuota cache is unavailable.': 'providerError.cacheUnavailable',
  'Enterprise usage data unavailable. Try again later.':
    'providerError.cursorEnterpriseUsageUnavailable',
  'Team request-based usage data unavailable. Try again later.':
    'providerError.cursorTeamRequestBasedUsageUnavailable',
  'Cursor request-based usage data unavailable. Try again later.':
    'providerError.cursorRequestBasedUsageUnavailable',
  'The refreshed Cursor login could not be saved.': 'providerError.cursorRefreshNotSaved',
  'The refreshed Claude credentials could not be saved.': [
    'providerError.refreshedCredentialsNotSaved',
    { provider: 'Claude' },
  ],
  'The refreshed Codex credentials could not be saved.': [
    'providerError.refreshedCredentialsNotSaved',
    { provider: 'Codex' },
  ],
  'The refreshed Grok credentials could not be saved.': [
    'providerError.refreshedCredentialsNotSaved',
    { provider: 'Grok' },
  ],
  'No active Cursor subscription.': 'providerError.noActiveCursorSubscription',
  'No active GLM Coding Plan. Subscribe at z.ai/subscribe to view usage.':
    'providerError.noActiveGlmPlan',
  'No active MiniMax token plan. Subscribe at minimax.io to view usage.':
    'providerError.noActiveMiniMaxPlan',
  'OpenCode Go subscription required.': 'providerError.openCodeGoSubscriptionRequired',
  'OpenCode was not detected. Sign in to OpenCode Go or use OpenCode locally first.':
    'providerError.openCodeNotDetected',
  'OpenCode login data could not be read. Sign in to OpenCode Go again.':
    'providerError.openCodeLoginUnreadable',
  'The OpenCode data directory could not be read.': 'providerError.openCodeDataDirUnreadable',
  'OpenCode Go login data is invalid or expired. Sign in to OpenCode Go again.':
    'providerError.openCodeGoLoginInvalid',
  'Start Antigravity or run `agy` and try again.': 'providerError.antigravityStartRequired',
  'Antigravity sign-in expired. Open Antigravity or run `agy` to refresh.':
    'providerError.antigravitySignInExpired',
  'Antigravity credentials could not be read from secure storage.':
    'providerError.antigravityCredentialsUnreadable',
  'Antigravity credentials are invalid. Sign in again in Antigravity or `agy`.':
    'providerError.antigravityCredentialsInvalid',
  'Antigravity usage is temporarily unavailable. Try again shortly.':
    'providerError.antigravityTemporarilyUnavailable',
  'Not logged in. Sign in via Cursor app or run `agent login`.': 'providerError.notLoggedInCursor',
  'Session expired. Sign in via Cursor app or run `agent login`.':
    'providerError.sessionExpiredCursor',
  'Token expired. Sign in via Cursor app or run `agent login`.': 'providerError.tokenExpiredCursor',
  'Grok is not logged in. Run `grok login`.': 'providerError.grokNotLoggedIn',
  'Grok login expired. Run `grok login` again.': 'providerError.grokLoginExpired',
  'Grok login data is invalid. Run `grok login` again.': 'providerError.grokLoginInvalid',
  'Devin is not logged in. Run `devin auth login` or sign in to the Devin app.':
    'providerError.devinNotLoggedIn',
  'Devin login expired. Run `devin auth login` or sign in to the Devin app.':
    'providerError.devinLoginExpired',
  'Devin quota data is unavailable for this account.': 'providerError.devinQuotaUnavailable',
  'Sign in to GitHub Copilot in your editor, or run `gh auth login`, and try again.':
    'providerError.copilotSignInRequired',
  'Your GitHub token is invalid or expired. Run `gh auth login` and try again.':
    'providerError.githubTokenInvalid',
  'Copilot usage data is unavailable for this account.': 'providerError.copilotUsageUnavailable',
  'Your Codex session expired. Run `codex` to sign in again.': 'providerError.codexSessionExpired',
  'Your Codex access token expired. Run `codex` to sign in again.':
    'providerError.codexTokenExpired',
  'Your Codex session was revoked. Run `codex` to sign in again.':
    'providerError.codexSessionRevoked',
  'Codex credentials changed while refreshing. Run `codex` to sign in again.':
    'providerError.codexCredentialsChanged',
  'Codex auth data is invalid. Run `codex` to sign in again.': 'providerError.codexAuthInvalid',
  'The Codex account changed while usage was refreshing. Refresh again.':
    'providerError.codexAccountChanged',
  'Your Claude session expired. Run `claude` to sign in again.':
    'providerError.claudeSessionExpired',
  'Your Claude token expired. Run `claude` to sign in again.': 'providerError.claudeTokenExpired',
  'Claude login changed during refresh. Refresh again.': 'providerError.claudeLoginChanged',
  'The Claude account changed while OpenQuota was running. Restart OpenQuota to reconnect it safely.':
    'providerError.claudeAccountChanged',
  'Claude OAuth settings contain an invalid URL.': 'providerError.claudeOAuthInvalidUrl',
  'Claude account settings could not be loaded.': 'providerError.claudeAccountSettingsUnavailable',
  'The Kimi API key could not be read or updated.': [
    'providerError.apiKeyUnreadable',
    { provider: 'Kimi' },
  ],
  'The MiniMax API key could not be read or updated.': [
    'providerError.apiKeyUnreadable',
    { provider: 'MiniMax' },
  ],
  'The OpenRouter API key could not be read or updated.': [
    'providerError.apiKeyUnreadable',
    { provider: 'OpenRouter' },
  ],
  'The Z.ai API key could not be read or updated.': [
    'providerError.apiKeyUnreadable',
    { provider: 'Z.ai' },
  ],
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
  // Pacing / deadline fallbacks emitted by pacing.ts and rendered via tBackend
  'Limit reached': 'metric.limitReached',
  'Reset unavailable': 'metric.resetUnavailable',
  // Service-level refresh state errors shown on provider cards
  'Provider refresh is temporarily unavailable.': 'providerError.refreshTemporarilyUnavailable',
  'Provider refresh stopped unexpectedly.': 'providerError.refreshStoppedUnexpectedly',
  'Provider refresh timed out.': 'providerError.refreshTimedOut',
  'The refreshed account state could not be saved.': 'providerError.accountStateCouldNotSave',
  'Usage refreshed, but the last successful snapshot could not be cached.':
    'providerError.snapshotCacheFailed',
  'The API key was saved securely, but OpenQuota could not finish updating provider status. Restart OpenQuota or try again.':
    'providerError.apiKeySaveIncomplete',
  'The API key was removed, but OpenQuota could not finish updating provider status. Restart OpenQuota or try again.':
    'providerError.apiKeyRemoveIncomplete',
  // Settings integration errors shown in the Settings notice
  'Launch at login status could not be read.': 'settings.launchAtLoginStatusUnavailable',
  'The saved global shortcut is currently unavailable.': 'settings.shortcutCurrentlyUnavailable',
  // Update failures returned by updates.rs (message/action pairs)
  'Another update operation is already running.': 'update.busy',
  'Wait for it to finish, then try again.': 'update.busyAction',
  'Automatic updates are not configured in this build.': 'update.notConfigured',
  'Download the latest version from the release page.': 'update.notConfiguredAction',
  'This Linux package cannot update itself.': 'update.manualInstallRequired',
  'Download the new package from the release page and install it normally.':
    'update.manualInstallAction',
  'OpenQuota is already up to date.': 'update.alreadyUpToDate',
  'No action is needed.': 'update.noActionNeeded',
  'GitHub refused the update download.': 'update.githubRefused',
  'Try again. If it still fails, download the verified installer from the release page.':
    'update.githubRefusedAction',
  'GitHub temporarily limited update requests.': 'update.githubRateLimited',
  'Wait a few minutes, then try again.': 'update.githubRateLimitedAction',
  'The downloaded update failed its security check.': 'update.signatureInvalid',
  'Do not install this download. Open the release page or try again later.':
    'update.signatureInvalidAction',
  'Check your connection or proxy, then try again.': 'update.networkFailedAction',
  'Try again or use the release page to download the installer manually.':
    'update.genericFailedAction',
};

export interface BackendPattern {
  pattern: RegExp;
  key: MessagePath;
  params: string[];
}

export const backendPatterns: BackendPattern[] = [
  // Provider errors returned by the Rust providers. Keep the more specific
  // usage/billing variants before the generic request variant.
  {
    pattern: /^Could not connect to (.+)\. Check your internet connection\.$/,
    key: 'providerError.couldNotConnect',
    params: ['provider'],
  },
  {
    pattern: /^Could not reach (.+)\. Check your internet connection\.$/,
    key: 'providerError.couldNotReach',
    params: ['provider'],
  },
  {
    pattern: /^(.+) usage request failed \(HTTP (\d+)\)\.$/,
    key: 'providerError.usageRequestFailedHttp',
    params: ['provider', 'code'],
  },
  {
    pattern: /^(.+) billing request failed \(HTTP (\d+)\)\.$/,
    key: 'providerError.billingRequestFailedHttp',
    params: ['provider', 'code'],
  },
  {
    pattern: /^(.+) request failed \(HTTP (\d+)\)\.$/,
    key: 'providerError.requestFailedHttp',
    params: ['provider', 'code'],
  },
  {
    pattern: /^(.+) usage data is temporarily unavailable\.$/,
    key: 'providerError.dataTemporarilyUnavailable',
    params: ['provider'],
  },
  {
    pattern: /^Local (.+) usage logs could not be processed\.$/,
    key: 'providerError.localLogsUnprocessable',
    params: ['provider'],
  },
  {
    pattern: /^Refreshed (.+) credentials could not be saved\.$/,
    key: 'providerError.refreshedCredentialsNotSaved',
    params: ['provider'],
  },
  {
    pattern: /^(.+) returned an invalid usage response\.$/,
    key: 'providerError.invalidUsageResponse',
    params: ['provider'],
  },
  {
    pattern: /^(.+) returned an invalid billing response\.$/,
    key: 'providerError.invalidBillingResponse',
    params: ['provider'],
  },
  {
    pattern: /^The (.+) API key is invalid\. Check it at (.+)\.$/,
    key: 'providerError.apiKeyInvalid',
    params: ['provider', 'url'],
  },
  {
    pattern: /^The (.+) API key could not be read or updated\.$/,
    key: 'providerError.apiKeyUnreadable',
    params: ['provider'],
  },
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
  // Update failures returned by updates.rs (dynamic operation strings)
  {
    pattern:
      /^OpenQuota could not (?:check for updates|install the signed update) because the network request failed\.$/,
    key: 'update.networkFailed',
    params: [],
  },
  {
    pattern: /^OpenQuota could not (?:check for updates|install the signed update)\.$/,
    key: 'update.genericFailed',
    params: [],
  },
  {
    pattern: /^The OpenQuota download page could not be opened: .+$/,
    key: 'update.downloadPageError',
    params: [],
  },
];
