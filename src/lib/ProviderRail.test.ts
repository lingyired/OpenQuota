import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ProviderCatalogIndex } from './metrics';
import ProviderRail from './ProviderRail.svelte';
import type { ProviderSnapshot, SettingsViewState, UsageViewState } from './types';
import { claudeState, codexState, providerCatalogIndex, settingsState } from '../test/appFixtures';

afterEach(() => {
  cleanup();
  Reflect.deleteProperty(Element.prototype, 'scrollIntoView');
});

function show(selectedProviderId = 'claude') {
  const settings: SettingsViewState['settings'] = {
    ...settingsState.settings,
    providers: [
      {
        id: 'claude',
        enabled: true,
        detected: true,
        expanded: false,
        metrics: [{ id: 'claude.session', enabled: true, section: 'alwaysVisible', pinned: true }],
      },
      settingsState.settings.providers[0],
      {
        id: 'antigravity',
        enabled: true,
        detected: true,
        expanded: false,
        metrics: [
          { id: 'antigravity.geminiPro', enabled: true, section: 'alwaysVisible', pinned: true },
        ],
      },
    ],
    taskbandProviders: {
      codex: {
        enabled: false,
        side: null,
        topColor: null,
        bottomColor: null,
        topBold: false,
        bottomBold: false,
        topSize: 12,
        bottomSize: 12,
        topAlign: 0,
        bottomAlign: 0,
        paddingLeft: 0,
        paddingRight: 0,
      },
    },
  };
  const viewState: UsageViewState = {
    providers: { claude: claudeState, codex: codexState },
  };
  const onSelect = vi.fn();
  render(ProviderRail, {
    viewState,
    settings,
    catalog: providerCatalogIndex,
    selectedProviderId,
    onSelect,
  });
  return { onSelect };
}

describe('ProviderRail', () => {
  it('renders every enabled provider and selects only provider tabs', () => {
    show('claude');
    expect(screen.getAllByRole('tab')).toHaveLength(3);
    expect(screen.queryByRole('tab', { name: 'All' })).not.toBeInTheDocument();
    expect(screen.getByRole('tab', { name: /Codex/ })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: /Antigravity/ })).toHaveTextContent('--');
    expect(screen.getByRole('tab', { name: /Claude.*Session.*80%/ })).toHaveAttribute(
      'aria-selected',
      'true',
    );
    expect(screen.getByRole('tab', { name: /Claude.*Session.*80%/ })).toBeInTheDocument();
    expect(
      screen.getByRole('tab', { name: /Codex.*Session.*68%.*Weekly.*41%/ }),
    ).toBeInTheDocument();
  });

  it('selects a provider on click and activates adjacent tabs with vertical arrow keys', async () => {
    const { onSelect } = show('claude');
    const claude = screen.getByRole('tab', { name: /Claude/ });
    await fireEvent.click(claude);
    expect(onSelect).toHaveBeenLastCalledWith('claude');

    await fireEvent.keyDown(claude, { key: 'ArrowDown' });
    expect(onSelect).toHaveBeenLastCalledWith('codex');
    expect(screen.getByRole('tab', { name: /Codex/ })).toHaveFocus();
    const codex = screen.getByRole('tab', { name: /Codex/ });
    await fireEvent.keyDown(codex, { key: 'End' });
    expect(onSelect).toHaveBeenLastCalledWith('antigravity');
    expect(screen.getByRole('tab', { name: /Antigravity/ })).toHaveFocus();
  });

  it('exposes a vertical tablist', () => {
    show();
    expect(screen.getByRole('tablist')).toHaveAttribute('aria-orientation', 'vertical');
  });

  it('scrolls the active provider square into view', async () => {
    const scrollIntoView = vi.fn();
    Object.defineProperty(Element.prototype, 'scrollIntoView', {
      configurable: true,
      value: scrollIntoView,
    });

    show('codex');

    await waitFor(() =>
      expect(scrollIntoView).toHaveBeenCalledWith({ block: 'nearest', inline: 'nearest' }),
    );
  });

  it('keeps only the selected tab tabbable', () => {
    show('codex');
    expect(screen.getByRole('tab', { name: /Codex/ })).toHaveAttribute('tabindex', '0');
    expect(screen.getByRole('tab', { name: /Claude/ })).toHaveAttribute('tabindex', '-1');
  });

  it('renders provider icons at the larger rail size', () => {
    show();
    expect(
      screen.getByRole('tab', { name: /Claude/ }).querySelector('.provider-icon'),
    ).toHaveAttribute('width', '22');
  });

  it('renders one short line per reading value without unit words', () => {
    const settings: SettingsViewState['settings'] = {
      ...settingsState.settings,
      providers: [
        {
          id: 'codex',
          enabled: true,
          detected: true,
          expanded: false,
          metrics: [{ id: 'codex.today', enabled: true, section: 'alwaysVisible', pinned: true }],
        },
      ],
    };
    render(ProviderRail, {
      viewState: { providers: { codex: codexState } },
      settings,
      catalog: providerCatalogIndex,
      selectedProviderId: 'codex',
      onSelect: vi.fn(),
    });

    const codex = screen.getByRole('tab', { name: /Codex.*Today.*\$3\.8 · 2\.1M tokens/ });
    const lines = Array.from(codex.querySelectorAll('.provider-rail__reading')).map(
      (line) => line.textContent,
    );
    expect(lines).toEqual(['$3.8', '2.1M']);
  });

  it('shrinks long readings instead of clipping them', () => {
    const catalog = new ProviderCatalogIndex({
      apiKeyProviderIds: [],
      providers: [
        {
          id: 'claude',
          displayName: 'Claude',
          shortName: 'Cl',
          fallbackEnabled: false,
          localUsageSourceNote: null,
          links: [],
          metrics: [
            {
              id: 'claude.status',
              label: 'Status',
              source: { kind: 'status', sourceId: 'status' },
              pinnable: true,
              defaultEnabled: true,
              defaultSection: 'alwaysVisible',
              defaultPinned: true,
              tray: { shortLabel: 'S', suffix: null },
            },
          ],
        },
      ],
    });
    const snapshot: ProviderSnapshot = {
      providerId: 'claude',
      plan: 'Pro',
      quotas: [],
      creditPackages: [],
      valueMetrics: [],
      statusMetrics: [{ id: 'status', label: 'Status', text: 'Unavailable', tone: 'warning' }],
      notices: [],
      usage: { today: null, yesterday: null, last30Days: null, daily: [], unknownModels: [] },
      refreshedAt: '2026-07-10T10:00:00Z',
      warnings: [],
    };
    const settings: SettingsViewState['settings'] = {
      ...settingsState.settings,
      providers: [
        {
          id: 'claude',
          enabled: true,
          detected: true,
          expanded: false,
          metrics: [{ id: 'claude.status', enabled: true, section: 'alwaysVisible', pinned: true }],
        },
      ],
    };
    render(ProviderRail, {
      viewState: {
        providers: {
          claude: {
            source: 'live',
            refreshing: false,
            stale: false,
            error: null,
            errorKind: null,
            lastAttemptAt: null,
            snapshot,
          },
        },
      },
      settings,
      catalog,
      selectedProviderId: 'codex',
      onSelect: vi.fn(),
    });

    const claude = screen.getByRole('tab', { name: /Claude.*Status.*Unavailable/ });
    const reading = claude.querySelector<HTMLElement>('.provider-rail__reading');
    expect(reading?.textContent).toBe('Unavailable');
    expect(Number.parseFloat(reading?.style.fontSize ?? '0')).toBeLessThan(11);
  });
});
