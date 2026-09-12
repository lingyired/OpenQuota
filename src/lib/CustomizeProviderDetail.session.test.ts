import { cleanup, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import CustomizeProviderDetail from './CustomizeProviderDetail.svelte';
import { ProviderCatalogIndex } from './metrics';
import type { AppSettings, ProviderCatalog } from './types';

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }));
vi.mock('@tauri-apps/api/event', () => ({ listen: mocks.listen }));

const catalogData: ProviderCatalog = {
  webviewAuthProviderIds: ['trae-cn'],
  providers: [
    {
      id: 'trae-cn',
      displayName: 'TraeWork CN',
      shortName: 'TR',
      fallbackEnabled: false,
      localUsageSourceNote: null,
      links: [{ label: 'Dashboard', url: 'https://www.trae.cn/account-setting#usage' }],
      metrics: [
        {
          id: 'trae-cn.credits',
          label: 'Credits',
          source: { kind: 'quota', sourceId: 'credits', sessionWindow: false },
          pinnable: true,
          defaultEnabled: true,
          defaultSection: 'alwaysVisible',
          defaultPinned: true,
          tray: { shortLabel: 'C', suffix: null },
        },
        {
          id: 'trae-cn.status',
          label: 'Status',
          source: { kind: 'status', sourceId: 'status' },
          pinnable: true,
          defaultEnabled: true,
          defaultSection: 'onDemand',
          defaultPinned: false,
          tray: { shortLabel: 'S', suffix: null },
        },
      ],
    },
  ],
};

const settings: AppSettings = {
  schemaVersion: 8,
  providers: [
    {
      id: 'trae-cn',
      enabled: false,
      detected: false,
      expanded: false,
      metrics: [
        { id: 'trae-cn.credits', enabled: true, section: 'alwaysVisible', pinned: true },
        { id: 'trae-cn.status', enabled: true, section: 'onDemand', pinned: false },
      ],
    },
  ],
  knownProviderIds: ['trae-cn'],
  providerNames: {},
  language: 'en',
  showTotalSpend: true,
  theme: 'system',
  density: 'default',
  reduceAnimations: false,
  windowMode: 'popup',
  menuBarStyle: 'text',
  usageDisplay: 'left',
  resetDisplay: 'countdown',
  timeFormat: 'system',
  alwaysShowPacing: false,
  launchAtLogin: false,
  autoCheckUpdates: true,
  dismissedUpdateVersion: null,
  lastUpdateCheckAt: null,
  globalShortcut: null,
  logLevel: 'info',
  notifications: { almostOut: false, cuttingItClose: false, willRunOut: false },
  totalSpendMetric: 'cost',
  totalSpendPeriod: 'today',
  detectionNoticeDismissed: true,
  taskband: {
    enabled: true,
    defaultSide: 'right',
    margin: 4,
    edgeMarginLeft: 0,
    edgeMarginRight: 0,
  },
  taskbandProviders: {},
};

describe('CustomizeProviderDetail session authentication', () => {
  beforeEach(() => {
    mocks.listen.mockReset().mockResolvedValue(vi.fn());
    mocks.invoke.mockReset().mockImplementation((command: string) => {
      if (command === 'get_provider_session_state') {
        return Promise.resolve({ providerId: 'trae-cn', status: 'notSet' });
      }
      return Promise.reject(new Error(`unexpected command ${command}`));
    });
  });

  afterEach(cleanup);

  it('renders WebView session controls instead of API-key controls when the provider declares that capability', async () => {
    render(CustomizeProviderDetail, {
      settings,
      providerId: 'trae-cn',
      catalog: new ProviderCatalogIndex(catalogData),
      renamableProviderIds: [],
      onChange: () => {},
      onNameChange: () => {},
      onReorderStart: () => {},
      onReorderEnd: () => {},
      reducedMotion: true,
    });

    expect(
      await screen.findByRole('region', { name: 'TraeWork CN Connection' }),
    ).toBeInTheDocument();
    expect(mocks.invoke).toHaveBeenCalledWith('get_provider_session_state', {
      providerId: 'trae-cn',
    });
    expect(mocks.invoke).not.toHaveBeenCalledWith('get_provider_api_key_state', {
      providerId: 'trae-cn',
    });
  });
});
