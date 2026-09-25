import { describe, expect, it } from 'vitest';
import { pinnedMetricLayouts, providerTabReadings } from './providerTabSummary';
import { ProviderCatalogIndex } from './metrics';
import type { AppSettings, ProviderLayout, ProviderSnapshot } from './types';
import { claudeState, codexState, providerCatalogIndex, settingsState } from '../test/appFixtures';

function provider(id: string, metrics: ProviderLayout['metrics']): ProviderLayout {
  return {
    id,
    enabled: true,
    detected: true,
    expanded: false,
    keychainAccessGranted: false,
    metrics,
  };
}

const usedSettings: AppSettings = { ...settingsState.settings, usageDisplay: 'used' };

const deepseekCatalog = new ProviderCatalogIndex({
  apiKeyProviderIds: ['deepseek'],
  providers: [
    {
      id: 'deepseek',
      displayName: 'DeepSeek',
      shortName: 'DS',
      fallbackEnabled: false,
      localUsageSourceNote: null,
      links: [],
      metrics: [
        {
          id: 'deepseek.balance',
          label: 'Balance',
          source: { kind: 'value', sourceId: 'balance' },
          pinnable: true,
          defaultEnabled: true,
          defaultSection: 'alwaysVisible',
          defaultPinned: true,
          tray: { shortLabel: 'B', suffix: null },
        },
      ],
    },
  ],
});

function deepseekBalances(wallets: [number, string][]): ProviderSnapshot {
  return {
    providerId: 'deepseek',
    plan: 'Available',
    quotas: [],
    creditPackages: [],
    valueMetrics: [
      {
        id: 'balance',
        label: 'Balance',
        values: wallets.map(([number, label]) => ({
          number,
          kind: 'currency' as const,
          label,
          estimated: false,
        })),
        expiriesAt: [],
      },
    ],
    statusMetrics: [],
    notices: [],
    usage: { today: null, yesterday: null, last30Days: null, daily: [], unknownModels: [] },
    refreshedAt: '2026-07-10T10:00:00Z',
    warnings: [],
  };
}

describe('provider tab summaries', () => {
  it('uses the first two pinned metrics even when a pinned metric is hidden in the dashboard', () => {
    const codex = provider('codex', [
      { id: 'codex.session', enabled: false, section: 'onDemand', pinned: true },
      { id: 'codex.weekly', enabled: true, section: 'alwaysVisible', pinned: true },
      { id: 'codex.spark', enabled: true, section: 'onDemand', pinned: true },
    ]);

    expect(pinnedMetricLayouts(codex, providerCatalogIndex).map((metric) => metric.id)).toEqual([
      'codex.session',
      'codex.weekly',
    ]);
    expect(
      providerTabReadings(codex, codexState.snapshot, usedSettings, providerCatalogIndex).map(
        (reading) => reading.reading,
      ),
    ).toEqual(['32%', '59%']);
  });

  it('follows the selected remaining or used display mode for quota readings', () => {
    const codex = provider('codex', [
      { id: 'codex.session', enabled: true, section: 'alwaysVisible', pinned: true },
      { id: 'codex.weekly', enabled: true, section: 'alwaysVisible', pinned: true },
    ]);

    expect(
      providerTabReadings(
        codex,
        codexState.snapshot,
        settingsState.settings,
        providerCatalogIndex,
      ).map((reading) => reading.reading),
    ).toEqual(['68%', '41%']);
  });

  it('formats value-backed quota metrics and keeps unavailable readings stable', () => {
    const extra = provider('claude', [
      { id: 'claude.extra', enabled: true, section: 'alwaysVisible', pinned: true },
    ]);
    const readings = providerTabReadings(
      extra,
      claudeState.snapshot,
      usedSettings,
      providerCatalogIndex,
    );
    expect(readings[0]).toMatchObject({ reading: '$12.5', available: true });

    expect(providerTabReadings(extra, null, usedSettings, providerCatalogIndex)).toEqual([
      { id: 'claude.extra', label: 'Extra Usage', reading: '--', lines: ['--'], available: false },
    ]);
  });

  it('keeps unit words out of the rail lines while the full reading stays accessible', () => {
    const today = provider('codex', [
      { id: 'codex.today', enabled: true, section: 'alwaysVisible', pinned: true },
    ]);

    expect(
      providerTabReadings(today, codexState.snapshot, usedSettings, providerCatalogIndex),
    ).toMatchObject([{ reading: '$3.8 · 2.1M tokens', lines: ['$3.8', '2.1M'] }]);
  });

  it('uses currency symbols and one line per wallet for balance readings', () => {
    const deepseek = provider('deepseek', [
      { id: 'deepseek.balance', enabled: true, section: 'alwaysVisible', pinned: true },
    ]);

    expect(
      providerTabReadings(
        deepseek,
        deepseekBalances([
          [110, 'CNY'],
          [3.25, 'USD'],
        ]),
        usedSettings,
        deepseekCatalog,
      ),
    ).toMatchObject([{ reading: '110 CNY · 3.3 USD', lines: ['¥110', '$3.3'] }]);
  });

  it('caps rail lines per metric and counts the hidden wallets', () => {
    const deepseek = provider('deepseek', [
      { id: 'deepseek.balance', enabled: true, section: 'alwaysVisible', pinned: true },
    ]);

    expect(
      providerTabReadings(
        deepseek,
        deepseekBalances([
          [110, 'CNY'],
          [3.25, 'USD'],
          [8.5, 'JPY'],
        ]),
        usedSettings,
        deepseekCatalog,
      ),
    ).toMatchObject([{ lines: ['¥110', '$3.3 +1'] }]);
  });

  it('keeps percent and dollar rail lines unchanged', () => {
    const codex = provider('codex', [
      { id: 'codex.session', enabled: true, section: 'alwaysVisible', pinned: true },
    ]);
    const extra = provider('claude', [
      { id: 'claude.extra', enabled: true, section: 'alwaysVisible', pinned: true },
    ]);

    expect(
      providerTabReadings(codex, codexState.snapshot, usedSettings, providerCatalogIndex),
    ).toMatchObject([{ reading: '32%', lines: ['32%'] }]);
    expect(
      providerTabReadings(extra, claudeState.snapshot, usedSettings, providerCatalogIndex),
    ).toMatchObject([{ reading: '$12.5', lines: ['$12.5'] }]);
  });

  it('formats a pinned usage metric with the same cost and token summary as the dashboard', () => {
    const today = provider('codex', [
      { id: 'codex.today', enabled: true, section: 'alwaysVisible', pinned: true },
    ]);
    expect(
      providerTabReadings(today, codexState.snapshot, usedSettings, providerCatalogIndex),
    ).toMatchObject([{ reading: '$3.8 · 2.1M tokens', available: true }]);
  });

  it('ignores pinned metrics that are not part of the provider catalog', () => {
    const unknown = provider('codex', [
      { id: 'codex.unknown', enabled: true, section: 'alwaysVisible', pinned: true },
    ]);
    expect(
      providerTabReadings(unknown, codexState.snapshot, usedSettings, providerCatalogIndex),
    ).toEqual([]);
  });
});
