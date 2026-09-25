import { cleanup, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import CustomizeProviderDetail from './CustomizeProviderDetail.svelte';
import { ProviderCatalogIndex } from './metrics';
import type { AppSettings, ProviderCatalog } from './types';

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }));
vi.mock('@tauri-apps/api/event', () => ({ listen: mocks.listen }));

const catalogData: ProviderCatalog = {
  webviewAuthProviderIds: ['trae-cn', 'deepseek', 'workbuddy-cn'],
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
    {
      id: 'deepseek',
      displayName: 'DeepSeek',
      shortName: 'DS',
      fallbackEnabled: false,
      localUsageSourceNote: null,
      links: [{ label: 'Dashboard', url: 'https://platform.deepseek.com/usage' }],
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
    {
      id: 'workbuddy-cn',
      displayName: 'Workbuddy CN',
      shortName: 'WB',
      fallbackEnabled: false,
      localUsageSourceNote: null,
      links: [{ label: 'Dashboard', url: 'https://workbuddy.example.test/usage' }],
      metrics: [
        {
          id: 'workbuddy-cn.quota',
          label: 'Quota',
          source: { kind: 'quota', sourceId: 'quota', sessionWindow: false },
          pinnable: true,
          defaultEnabled: true,
          defaultSection: 'alwaysVisible',
          defaultPinned: true,
          tray: { shortLabel: 'Q', suffix: null },
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
      keychainAccessGranted: false,
      metrics: [
        { id: 'trae-cn.credits', enabled: true, section: 'alwaysVisible', pinned: true },
        { id: 'trae-cn.status', enabled: true, section: 'onDemand', pinned: false },
      ],
    },
    {
      id: 'deepseek',
      enabled: false,
      detected: false,
      expanded: false,
      keychainAccessGranted: false,
      metrics: [
        {
          id: 'deepseek.balance',
          enabled: true,
          section: 'alwaysVisible',
          pinned: true,
        },
      ],
    },
    {
      id: 'workbuddy-cn',
      enabled: false,
      detected: false,
      expanded: false,
      keychainAccessGranted: false,
      metrics: [
        { id: 'workbuddy-cn.quota', enabled: true, section: 'alwaysVisible', pinned: true },
      ],
    },
  ],
  knownProviderIds: ['trae-cn', 'deepseek', 'workbuddy-cn'],
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
    mocks.invoke
      .mockReset()
      .mockImplementation((command: string, args?: { providerId: string }) => {
        if (command === 'get_provider_session_state') {
          return Promise.resolve({ providerId: args?.providerId, status: 'notSet' });
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

  it('renders WebView session controls for DeepSeek instead of API-key controls', async () => {
    render(CustomizeProviderDetail, {
      settings,
      providerId: 'deepseek',
      catalog: new ProviderCatalogIndex(catalogData),
      renamableProviderIds: [],
      onChange: () => {},
      onNameChange: () => {},
      onReorderStart: () => {},
      onReorderEnd: () => {},
      reducedMotion: true,
    });

    expect(await screen.findByRole('region', { name: 'DeepSeek Connection' })).toBeInTheDocument();
    expect(mocks.invoke).toHaveBeenCalledWith('get_provider_session_state', {
      providerId: 'deepseek',
    });
    expect(mocks.invoke).not.toHaveBeenCalledWith('get_provider_api_key_state', {
      providerId: 'deepseek',
    });
  });

  it('renders the WebView session controls for WorkBuddy instead of API-key controls', async () => {
    render(CustomizeProviderDetail, {
      settings,
      providerId: 'workbuddy-cn',
      catalog: new ProviderCatalogIndex(catalogData),
      renamableProviderIds: [],
      onChange: () => {},
      onNameChange: () => {},
      onReorderStart: () => {},
      onReorderEnd: () => {},
      reducedMotion: true,
    });

    // WorkBuddy 复用 Trae 那套 webview 登录：Open Sign-In → 关窗抓取 → Connected。
    expect(
      await screen.findByRole('region', { name: 'Workbuddy CN Connection' }),
    ).toBeInTheDocument();
    expect(await screen.findByRole('button', { name: 'Open Sign-In' })).toBeInTheDocument();
    expect(mocks.invoke).toHaveBeenCalledWith('get_provider_session_state', {
      providerId: 'workbuddy-cn',
    });
    expect(mocks.invoke).not.toHaveBeenCalledWith('get_provider_api_key_state', {
      providerId: 'workbuddy-cn',
    });
  });
});
