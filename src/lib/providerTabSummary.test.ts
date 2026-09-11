import { describe, expect, it } from 'vitest';
import { pinnedMetricLayouts, providerTabReadings } from './providerTabSummary';
import type { AppSettings, ProviderLayout } from './types';
import { claudeState, codexState, providerCatalogIndex, settingsState } from '../test/appFixtures';

function provider(id: string, metrics: ProviderLayout['metrics']): ProviderLayout {
  return { id, enabled: true, detected: true, expanded: false, metrics };
}

const usedSettings: AppSettings = { ...settingsState.settings, usageDisplay: 'used' };

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
    expect(readings[0]).toMatchObject({ reading: '$12.50', available: true });

    expect(providerTabReadings(extra, null, usedSettings, providerCatalogIndex)).toEqual([
      { id: 'claude.extra', label: 'Extra Usage', reading: '--', available: false },
    ]);
  });

  it('formats a pinned usage metric with the same cost and token summary as the dashboard', () => {
    const today = provider('codex', [
      { id: 'codex.today', enabled: true, section: 'alwaysVisible', pinned: true },
    ]);
    expect(
      providerTabReadings(today, codexState.snapshot, usedSettings, providerCatalogIndex),
    ).toMatchObject([{ reading: '$3.84 · 2.1M tokens', available: true }]);
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
