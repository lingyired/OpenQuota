import { describe, expect, it } from 'vitest';
import layoutCss from '../styles/layout.css?raw';
import sharedComponentCss from '../styles/components.css?raw';
import tokensCss from '../styles/tokens.css?raw';
import customizeDetail from './CustomizeProviderDetail.svelte?raw';
import customizeList from './CustomizeProviderList.svelte?raw';
import dashboard from './Dashboard.svelte?raw';
import providerNameSection from './ProviderNameSection.svelte?raw';
import settings from './SettingsScreen.svelte?raw';
import { coLocatedComponentCss } from './uiStyleSources';
import { en } from './i18n/messages/en';

const css = `${tokensCss}\n${layoutCss}\n${sharedComponentCss}\n${coLocatedComponentCss}`;

describe('native UI language contract', () => {
  it('uses the platform system font and reference type sizes', () => {
    expect(css).toMatch(/font-family:\s*system-ui,/);
    expect(css).not.toMatch(/font-family:\s*Inter/);
    expect(css).toMatch(/\.provider-header h1\s*{[^}]*font-size: 14px;[^}]*font-weight: 600;/s);
    expect(css).toMatch(/\.provider-list-main b\s*{[^}]*font-size: 14px;[^}]*font-weight: 600;/s);
    expect(css).toMatch(/\.setting-row\s*{[^}]*font-size: 13px;/s);
  });

  it('keeps the critical flame colored while its warning copy stays secondary', () => {
    expect(css).not.toMatch(/\.metric__heading span\s*{/);
    expect(css).toMatch(
      /\.metric__heading \.pace-warning__icon\s*{[^}]*color: var\(--meter-critical\);/s,
    );
    expect(css).toMatch(/\.metric__heading \.pace-warning\s*{[^}]*color: var\(--secondary\);/s);
  });

  it('keeps spend providers visually distinct in both appearances', () => {
    for (const provider of ['claude', 'codex', 'cursor', 'grok', 'opencode', 'openrouter']) {
      expect(tokensCss).toContain(`--provider-${provider}:`);
    }
    expect(tokensCss).toMatch(
      /@media \(prefers-color-scheme: dark\)[\s\S]*--provider-cursor: #f5f5f7;[\s\S]*--provider-opencode: #aeaeb2;/,
    );
    expect(tokensCss).toMatch(
      /:root\[data-theme='dark'\][\s\S]*--provider-cursor: #f5f5f7;[\s\S]*--provider-opencode: #aeaeb2;/,
    );
  });

  it('keeps Customize concise and free of duplicate status and count copy', () => {
    expect(customizeList).toContain("$tStore('customize.settingsDesc')");
    expect(en.customize.settingsDesc).toBe('Notifications, appearance and more');
    expect(customizeList).toContain(
      "$tStore('customize.metricCount', { count: provider.metrics.length })",
    );
    expect(en.customize.metricCount).toBe('{count} metrics');
    expect(customizeList).not.toContain('Detected locally');
    expect(customizeList).not.toContain('screen-intro');
    expect(customizeList).not.toContain('pinned\n');
    expect(customizeDetail).toContain("$tStore('customize.dragMetricsHere')");
    expect(en.customize.dragMetricsHere).toBe('Drag metrics here');
    expect(customizeDetail).toContain('customize.starredForMenuBar');
    expect(en.customize.starredForMenuBar).toBe('Starred for menu bar');
    expect(customizeDetail).toContain('customize.removedFromMenuBar');
    expect(en.customize.removedFromMenuBar).toBe('Removed from menu bar');
    expect(customizeDetail).toContain('customize.upTo2Stars');
    expect(en.customize.upTo2Stars).toBe('Up to 2 stars per provider');
    expect(customizeDetail).not.toContain('provider-toggle-row');
    expect(customizeDetail).not.toContain('section-divider');
    expect(customizeDetail).not.toContain('of 2 pinned');
  });

  it('uses the shared Settings labels and single-line control rows', () => {
    const settingsLabels: Record<string, string> = {
      'settings.general': 'General',
      'settings.showTotalSpend': 'Show Total Spend',
      'settings.launchAtLogin': 'Launch at Login',
      'settings.globalShortcut': 'Global Shortcut',
      'settings.iconStyle': 'Icon Style',
      'settings.appearance': 'Appearance',
      'settings.windowMode': 'Window Mode',
      'settings.usageDisplay': 'Usage Display',
      'settings.notifications': 'Notifications',
      'settings.advanced': 'Advanced',
      'settings.updates': 'Updates',
      'settings.checkForUpdatesAutomatically': 'Check for Updates Automatically',
      'settings.checkForUpdates': 'Check for Updates…',
    };
    for (const [key, label] of Object.entries(settingsLabels)) {
      expect(settings).toContain(`$tStore('${key}')`);
      expect(en.settings[key.replace('settings.', '') as keyof typeof en.settings]).toBe(label);
    }
    expect(settings).toContain("{ value: 'system', label: $tStore('settings.languageAuto') }");
    expect(en.settings.languageAuto).toBe('Auto');
    expect(settings).toContain("{ value: 'twelveHour', label: $tStore('settings.twelveHour') }");
    expect(settings).toContain(
      "{ value: 'twentyFourHour', label: $tStore('settings.twentyFourHour') }",
    );
    expect(settings).not.toContain('<h2>Startup</h2>');
    expect(settings).not.toContain('Automatic Checks');
    expect(settings).not.toContain('Combined cost and token summary.');
    expect(settings).not.toContain('Show projections even when usage is healthy.');
    expect(settings).not.toContain('>×</button');
  });

  it('keeps dashboard onboarding, empty state, and menus on the shared wording', () => {
    const dashboardLabels: Record<string, string> = {
      'dashboard.welcome': 'Welcome to OpenQuota',
      'dashboard.openCustomize': 'Open Customize',
      'dashboard.empty': 'Turn on Customize to choose what to show.',
      'dashboard.customize': 'Customize…',
      'dashboard.rename': 'Rename…',
      'dashboard.shareScreenshot': 'Share Screenshot',
      'dashboard.hideMetric': 'Hide',
      'dashboard.unstar': 'Unstar',
      'dashboard.starForMenuBar': 'Star for menu bar',
      'dashboard.refreshing': 'Refreshing',
    };
    for (const [key, label] of Object.entries(dashboardLabels)) {
      expect(dashboard).toContain(`$tStore('${key}')`);
      expect(en.dashboard[key.replace('dashboard.', '') as keyof typeof en.dashboard]).toBe(label);
    }
    expect(dashboard).toContain("$tStore('dashboard.refreshProvider', {");
    expect(en.dashboard.refreshProvider).toBe('Refresh {provider}');
    expect(dashboard).not.toContain('Providers Detected');
    expect(dashboard).not.toContain('Starter Provider');
    expect(dashboard).not.toContain("Expand'} On Demand");
    expect(dashboard).not.toContain('>×</button');
  });

  it('keeps interactive highlights in the component layer that owns their base style', () => {
    expect(providerNameSection).toMatch(
      /\.provider-name-card:focus-within\s*{[^}]*box-shadow: inset 0 0 0 2px/s,
    );
    expect(providerNameSection).toMatch(/input\s*{[^}]*display: block;/s);
    expect(dashboard).toMatch(
      /\.context-menu button:not\(:disabled\):hover,[\s\S]*background: var\(--button-hover\);/,
    );
    expect(sharedComponentCss).not.toContain('.context-menu button:hover');
  });
});
