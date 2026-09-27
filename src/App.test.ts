import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import App from './App.svelte';
import type { PanelHeightMode } from './lib/backend';
import type {
  AppSettings,
  ProviderCatalog,
  ProviderViewState,
  SettingsViewState,
  UsageViewState,
} from './lib/types';
import {
  antigravityState,
  claudeState,
  codexState,
  liveState,
  providerCatalog,
  settingsState,
} from './test/appFixtures';

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(),
  currentMonitor: vi.fn(),
  windowLabel: 'main',
  startDragging: vi.fn(),
  startResizeDragging: vi.fn(),
}));
const appEventHandlers = new Map<string, (payload: unknown) => void>();
vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }));
vi.mock('@tauri-apps/api/event', () => ({ listen: mocks.listen }));
vi.mock('@tauri-apps/api/window', () => ({
  currentMonitor: mocks.currentMonitor,
  getCurrentWindow: () => ({
    label: mocks.windowLabel,
    scaleFactor: () => Promise.resolve(1),
    innerSize: () => Promise.resolve({ width: 320, height: 600 }),
    startDragging: mocks.startDragging,
    startResizeDragging: mocks.startResizeDragging,
  }),
}));

type InvokeArgs = {
  settings?: SettingsViewState['settings'];
  providerId?: string;
  linkIndex?: number;
  height?: number;
};
type InvokeImplementation = (command: string, args?: InvokeArgs) => unknown;

function mockInvoke(
  implementation: InvokeImplementation,
  catalog: ProviderCatalog = providerCatalog,
) {
  mocks.invoke.mockImplementation((command: string, args?: InvokeArgs) => {
    if (command === 'get_bootstrap_state') {
      return Promise.all([
        implementation('get_usage_state', args),
        implementation('get_app_settings', args),
      ]).then(([usage, settings]) => ({ usage, settings, catalog }));
    }
    return implementation(command, args);
  });
}

async function mountSettingsWindow(targetScreen: string) {
  cleanup();
  mocks.windowLabel = 'settings';
  appEventHandlers.clear();
  render(App);
  await waitFor(() => expect(document.querySelector('[data-settings-workspace]')).toBeTruthy());
  if (targetScreen !== 'settings') {
    await waitFor(() => expect(appEventHandlers.get('settings-workspace-selection')).toBeDefined());
    appEventHandlers.get('settings-workspace-selection')?.({ payload: targetScreen });
    await waitFor(() =>
      expect(document.querySelector('.screen-page[data-screen="settings"]')).toBeTruthy(),
    );
  }
}

async function openSettingsWorkspace(targetScreen = 'settings') {
  if (mocks.windowLabel !== 'settings') {
    if (targetScreen === 'settings') {
      await fireEvent.click(screen.getByRole('button', { name: 'Open Settings' }));
    } else {
      await fireEvent.keyDown(document, { key: 'Enter' });
    }
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith('open_settings_window', { target: targetScreen }),
    );
    await mountSettingsWindow(targetScreen);
  }
  await waitFor(() => expect(document.querySelector('[data-settings-workspace]')).toBeTruthy());
}

function webviewAuthFixture() {
  const catalog: ProviderCatalog = {
    apiKeyProviderIds: [],
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
  const settings = structuredClone(settingsState);
  settings.settings.knownProviderIds = ['trae-cn'];
  settings.settings.providers = [
    {
      id: 'trae-cn',
      enabled: true,
      detected: false,
      expanded: false,
      useProxy: false,
      metrics: [
        { id: 'trae-cn.credits', enabled: true, section: 'alwaysVisible', pinned: true },
        { id: 'trae-cn.status', enabled: true, section: 'onDemand', pinned: false },
      ],
    },
  ];
  const usage: UsageViewState = {
    providers: {
      'trae-cn': {
        snapshot: null,
        source: 'none',
        refreshing: false,
        stale: false,
        error: 'Sign in to TraeWork CN to view usage.',
        errorKind: 'authentication',
        lastAttemptAt: null,
      },
    },
    lastFullRefreshAt: null,
  };
  return { catalog, settings, usage };
}

function deviceCodeSignInFixture(errorKind: ProviderViewState['errorKind'] = 'authentication') {
  const catalog: ProviderCatalog = {
    apiKeyProviderIds: [],
    webviewAuthProviderIds: [],
    deviceCodeSignInProviderIds: ['workbuddy-cn'],
    providers: [
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
  const settings = structuredClone(settingsState);
  settings.settings.knownProviderIds = ['workbuddy-cn'];
  settings.settings.providers = [
    {
      id: 'workbuddy-cn',
      enabled: true,
      detected: false,
      expanded: false,
      useProxy: false,
      metrics: [
        { id: 'workbuddy-cn.quota', enabled: true, section: 'alwaysVisible', pinned: true },
      ],
    },
  ];
  const usage: UsageViewState = {
    providers: {
      'workbuddy-cn': {
        snapshot: null,
        source: 'none',
        refreshing: false,
        stale: false,
        error: 'Sign in to Workbuddy CN to view usage.',
        errorKind,
        lastAttemptAt: null,
      },
    },
    lastFullRefreshAt: null,
  };
  return { catalog, settings, usage };
}

describe('Quota01 dashboard', () => {
  beforeEach(() => {
    localStorage.clear();
    mocks.windowLabel = 'main';
    appEventHandlers.clear();
    mocks.currentMonitor.mockResolvedValue({
      scaleFactor: 1,
      workArea: { size: { width: 1280, height: 700 } },
    });
    mocks.listen
      .mockReset()
      .mockImplementation((event: string, handler: (payload: unknown) => void) => {
        appEventHandlers.set(event, handler);
        return Promise.resolve(vi.fn());
      });
    mocks.startDragging.mockReset().mockResolvedValue(undefined);
    mocks.startResizeDragging.mockReset().mockResolvedValue(undefined);
    mocks.invoke.mockReset();
    mockInvoke((command: string, args?: InvokeArgs) => {
      if (
        command === 'get_usage_state' ||
        command === 'refresh_usage' ||
        command === 'refresh_provider_usage'
      )
        return Promise.resolve(liveState);
      if (command === 'get_app_settings') return Promise.resolve(settingsState);
      if (command === 'save_app_settings')
        return Promise.resolve({
          ...settingsState,
          settings: args?.settings ?? settingsState.settings,
        });
      if (command === 'request_notification_permission')
        return Promise.resolve({ ...settingsState, notificationPermission: 'granted' });
      if (command === 'open_notification_settings') return Promise.resolve();
      if (command === 'open_provider_link') return Promise.resolve();
      if (command === 'reset_customization') return Promise.resolve(settingsState);
      if (command === 'reset_all_settings')
        return Promise.resolve({ ...settingsState, settingsRevision: 1 });
      if (command === 'reset_provider_customization') return Promise.resolve(settingsState);
      if (command === 'get_panel_resize_edge') return Promise.resolve('bottom');
      if (command === 'get_panel_height_mode') return Promise.resolve('automatic');
      if (command === 'fit_panel_to_content') return Promise.resolve(true);
      if (command === 'set_panel_height_automatic') return Promise.resolve();
      if (command === 'set_panel_height_manual') return Promise.resolve();
      if (command === 'begin_panel_resize') return Promise.resolve('bottom');
      if (command === 'lock_panel_resize_axis') return Promise.resolve();
      if (command === 'get_log_path') return Promise.resolve('C:\\Quota01\\logs\\Quota01.log');
      if (command === 'open_log_folder') return Promise.resolve();
      if (command === 'dismiss_main_window') return Promise.resolve();
      if (command === 'check_for_updates')
        return Promise.resolve({
          available: false,
          currentVersion: '0.1.0',
          version: null,
          body: null,
          installable: true,
          releaseUrl: 'https://github.com/deviffyy/OpenQuota/releases/latest',
        });
      return Promise.reject(new Error(`unexpected command ${command}`));
    });
  });
  afterEach(() => {
    cleanup();
    localStorage.clear();
  });

  it('renders quota, total spend, and the 30-day trend from backend data', async () => {
    const { container } = render(App);
    expect(await screen.findByText('Plus')).toBeInTheDocument();
    expect(screen.getByRole('progressbar', { name: 'Session used' })).toHaveAttribute(
      'aria-valuenow',
      '32',
    );
    expect(screen.getByRole('progressbar', { name: 'Weekly used' })).toBeInTheDocument();
    expect(screen.getByRole('region', { name: 'Total Spend' })).toBeInTheDocument();
    expect(screen.getByRole('region', { name: 'Usage Trend' })).toBeInTheDocument();
    expect(container.querySelector('.spend-ring__label')).toHaveAttribute(
      'data-tooltip',
      '$3.8 · Estimated locally, so it may be off',
    );
    expect(screen.getByText(`Quota01 ${import.meta.env.APP_VERSION}`)).toBeInTheDocument();
    expect(container.querySelector('.floating-chrome')).not.toBeInTheDocument();
  });

  it('exposes Settings and screenshot sharing as direct footer buttons without dropdowns', async () => {
    const { container } = render(App);
    await screen.findByText('Plus');

    expect(container.querySelector('.options-menu')).not.toBeInTheDocument();
    expect(container.querySelector('.share-menu')).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Open Settings' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Share Screenshot' })).toBeInTheDocument();
    expect(container.querySelectorAll('.footer-actions > button')).toHaveLength(2);

    await fireEvent.click(screen.getByRole('button', { name: 'Open Settings' }));
    expect(mocks.invoke).toHaveBeenCalledWith('open_settings_window', { target: 'settings' });
    expect(screen.getByRole('main', { name: 'Quota01 usage dashboard' })).toBeInTheDocument();
  });

  it('offers a customization action when no provider is enabled', async () => {
    const emptySettings: SettingsViewState = {
      ...settingsState,
      settings: { ...settingsState.settings, providers: [], detectionNoticeDismissed: true },
    };
    mockInvoke((command: string) => {
      if (command === 'get_usage_state') return Promise.resolve({ providers: {} });
      if (command === 'get_app_settings') return Promise.resolve(emptySettings);
      if (command === 'check_for_updates')
        return Promise.resolve({
          available: false,
          currentVersion: '0.1.0',
          version: null,
          body: null,
          installable: true,
          releaseUrl: 'https://github.com/deviffyy/OpenQuota/releases/latest',
        });
      return Promise.resolve();
    });

    render(App);
    expect(await screen.findByRole('button', { name: 'Open Customize' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Share Screenshot' })).toBeDisabled();
    expect(screen.queryByRole('tab')).not.toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Open Customize' }));
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith('open_settings_window', { target: 'customize' }),
    );
    expect(screen.getByRole('main', { name: 'Quota01 usage dashboard' })).toBeInTheDocument();
    expect(screen.queryByRole('heading', { name: 'Customize' })).not.toBeInTheDocument();
  });

  it('opens Settings through the keyboard shortcut without changing the popup page', async () => {
    render(App);
    await screen.findByText('Plus');

    await fireEvent.keyDown(document, { key: ',', metaKey: true });

    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith('open_settings_window', { target: 'settings' }),
    );
    expect(screen.getByRole('main', { name: 'Quota01 usage dashboard' })).toBeInTheDocument();
    expect(document.querySelector('[data-settings-workspace]')).toBeNull();
  });

  it('provides a native drag surface and hide control in floating window mode', async () => {
    Object.defineProperty(window, '__TAURI_INTERNALS__', {
      configurable: true,
      value: {},
    });
    const userAgent = navigator.userAgent;
    Object.defineProperty(navigator, 'userAgent', {
      configurable: true,
      value: 'Mozilla/5.0 (Windows NT 10.0; Win64; x64)',
    });
    const defaultInvoke = mocks.invoke.getMockImplementation()!;
    mockInvoke((command: string, args?: InvokeArgs) => {
      if (command === 'get_app_settings')
        return Promise.resolve({
          ...settingsState,
          settings: { ...settingsState.settings, windowMode: 'floating' as const },
        });
      if (command === 'get_panel_resize_edge') return new Promise(() => undefined);
      return defaultInvoke(command, args);
    });

    try {
      const { container } = render(App);
      await screen.findByText('Plus');
      const dragSurface = container.querySelector<HTMLElement>('.floating-chrome__drag');
      expect(dragSurface).toBeInTheDocument();
      expect(screen.getByRole('button', { name: 'Hide Quota01' })).toBeInTheDocument();
      expect(screen.getByRole('separator', { name: 'Resize panel height' })).toHaveClass(
        'panel-resize-dragger--bottom',
      );

      await fireEvent.pointerDown(dragSurface!, { button: 0 });
      expect(mocks.startDragging).toHaveBeenCalledOnce();
    } finally {
      delete (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
      Object.defineProperty(navigator, 'userAgent', { configurable: true, value: userAgent });
    }
  });

  it('refreshes the resize edge after switching from floating window to tray popup', async () => {
    Object.defineProperty(window, '__TAURI_INTERNALS__', {
      configurable: true,
      value: {},
    });
    const userAgent = navigator.userAgent;
    Object.defineProperty(navigator, 'userAgent', {
      configurable: true,
      value: 'Mozilla/5.0 (Windows NT 10.0; Win64; x64)',
    });
    let appliedMode: AppSettings['windowMode'] = 'floating';
    const defaultInvoke = mocks.invoke.getMockImplementation()!;
    mockInvoke((command: string, args?: InvokeArgs) => {
      if (command === 'get_app_settings')
        return Promise.resolve({
          ...settingsState,
          settings: { ...settingsState.settings, windowMode: 'floating' as const },
        });
      if (command === 'save_app_settings') {
        appliedMode = args?.settings?.windowMode ?? appliedMode;
        return Promise.resolve({
          ...settingsState,
          settings: args?.settings ?? settingsState.settings,
        });
      }
      if (command === 'get_panel_resize_edge')
        return Promise.resolve(appliedMode === 'floating' ? 'bottom' : 'top');
      return defaultInvoke(command, args);
    });

    try {
      render(App);
      await screen.findByText('Plus');
      expect(screen.getByRole('separator', { name: 'Resize panel height' })).toHaveClass(
        'panel-resize-dragger--bottom',
      );
      await openSettingsWorkspace();
      await fireEvent.click(screen.getByRole('combobox', { name: 'Window Mode' }));
      await fireEvent.click(screen.getByRole('option', { name: 'Tray Popup' }));

      expect(
        screen.queryByRole('separator', { name: 'Resize panel height' }),
      ).not.toBeInTheDocument();
    } finally {
      delete (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
      Object.defineProperty(navigator, 'userAgent', { configurable: true, value: userAgent });
    }
  });

  it('shows enabled provider summaries while switching the selected data view', async () => {
    const multiProviderSettings = {
      ...settingsState,
      settings: {
        ...settingsState.settings,
        providers: [
          {
            id: 'claude',
            enabled: true,
            detected: true,
            expanded: false,
            useProxy: false,
            metrics: [
              {
                id: 'claude.session',
                enabled: true,
                section: 'alwaysVisible' as const,
                pinned: true,
              },
              {
                id: 'claude.extra',
                enabled: true,
                section: 'alwaysVisible' as const,
                pinned: false,
              },
            ],
          },
          ...settingsState.settings.providers,
          {
            id: 'antigravity',
            enabled: true,
            detected: true,
            expanded: false,
            useProxy: false,
            metrics: [
              {
                id: 'antigravity.geminiPro',
                enabled: true,
                section: 'alwaysVisible' as const,
                pinned: true,
              },
              {
                id: 'antigravity.geminiWeekly',
                enabled: true,
                section: 'alwaysVisible' as const,
                pinned: true,
              },
            ],
          },
        ],
      },
    };
    mockInvoke((command: string) => {
      if (command === 'get_usage_state')
        return Promise.resolve({
          providers: { claude: claudeState, codex: codexState, antigravity: antigravityState },
        });
      if (command === 'get_app_settings') return Promise.resolve(multiProviderSettings);
      if (command === 'check_for_updates')
        return Promise.resolve({
          available: false,
          currentVersion: '0.1.0',
          version: null,
          body: null,
          installable: true,
          releaseUrl: 'https://github.com/deviffyy/OpenQuota/releases/latest',
        });
      return Promise.resolve(multiProviderSettings);
    });

    render(App);
    expect(await screen.findByRole('heading', { name: 'Claude' })).toBeInTheDocument();
    expect(screen.queryByRole('heading', { name: 'Antigravity' })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: '$37.5 left' })).toBeInTheDocument();
    expect(screen.getAllByRole('tab')).toHaveLength(3);
    expect(screen.getByRole('tab', { name: /Codex.*68%/ })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: /Antigravity.*100%/ })).toBeInTheDocument();
    expect(screen.getAllByRole('progressbar')).toHaveLength(2);
    expect(
      within(screen.getByRole('region', { name: 'Total Spend' })).getByRole('img', {
        name: 'Only includes Claude and Codex',
      }),
    ).toBeInTheDocument();

    const claudeTab = screen.getByRole('tab', { name: /Claude/ });
    expect(claudeTab).toHaveAttribute('aria-selected', 'true');
    await fireEvent.click(screen.getByRole('tab', { name: /Antigravity/ }));
    await waitFor(() => {
      expect(screen.getByRole('tab', { name: /Antigravity/ })).toHaveAttribute(
        'aria-selected',
        'true',
      );
      expect(screen.getByRole('heading', { name: 'Antigravity' })).toBeInTheDocument();
      expect(screen.queryByRole('group', { name: 'Claude provider' })).not.toBeInTheDocument();
    });
    expect(screen.getByRole('region', { name: 'Total Spend' })).toBeInTheDocument();
  });

  it('renames an observed Claude card from its context menu', async () => {
    const claudeSettings: SettingsViewState = {
      ...settingsState,
      settings: {
        ...settingsState.settings,
        providers: [
          {
            id: 'claude',
            enabled: true,
            detected: true,
            expanded: false,
            useProxy: false,
            metrics: [
              { id: 'claude.session', enabled: true, section: 'alwaysVisible', pinned: true },
            ],
          },
        ],
      },
    };
    const claudeUsage: UsageViewState = { providers: { claude: claudeState } };
    mockInvoke((command: string, args?: InvokeArgs) => {
      if (
        command === 'get_usage_state' ||
        command === 'refresh_usage' ||
        command === 'refresh_provider_usage'
      )
        return Promise.resolve(claudeUsage);
      if (command === 'get_app_settings') return Promise.resolve(claudeSettings);
      if (command === 'save_app_settings')
        return Promise.resolve({
          ...claudeSettings,
          settings: args?.settings ?? claudeSettings.settings,
        });
      if (command === 'get_panel_resize_edge') return Promise.resolve('bottom');
      if (command === 'get_panel_height_mode') return Promise.resolve('automatic');
      if (command === 'fit_panel_to_content') return Promise.resolve(true);
      if (command === 'check_for_updates')
        return Promise.resolve({
          available: false,
          currentVersion: '0.1.0',
          version: null,
          body: null,
          installable: true,
          releaseUrl: 'https://github.com/deviffyy/OpenQuota/releases/latest',
        });
      return Promise.resolve();
    });

    render(App);
    const provider = await screen.findByRole('group', { name: 'Claude provider' });
    await fireEvent.contextMenu(provider);
    await fireEvent.click(screen.getByRole('menuitem', { name: 'Rename…' }));
    const dialog = screen.getByRole('dialog', { name: 'Rename Card' });
    const input = within(dialog).getByRole('textbox', { name: 'Name' });
    await fireEvent.input(input, { target: { value: 'Personal' } });
    await fireEvent.click(within(dialog).getByRole('button', { name: 'Rename' }));

    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith('save_app_settings', {
        expectedSettingsRevision: 0,
        expectedAccountRevision: 0,
        settings: expect.objectContaining({ providerNames: { claude: 'Personal' } }),
      }),
    );
    expect(await screen.findByRole('heading', { name: 'Personal' })).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Move Personal' })).toHaveFocus(),
    );
    const savesAfterRename = mocks.invoke.mock.calls.filter(
      ([command]) => command === 'save_app_settings',
    ).length;
    await fireEvent.keyDown(document, { key: 'z', ctrlKey: true });
    await Promise.resolve();
    expect(
      mocks.invoke.mock.calls.filter(([command]) => command === 'save_app_settings'),
    ).toHaveLength(savesAfterRename);
  });

  it('renames an observed extra Claude account card', async () => {
    const providerId = 'claude@1234abcd';
    const extraCatalog = structuredClone(providerCatalog);
    const baseDefinition = extraCatalog.providers.find((provider) => provider.id === 'claude')!;
    extraCatalog.providers.push({
      ...baseDefinition,
      id: providerId,
      displayName: 'Claude — Work',
      fallbackEnabled: false,
      metrics: baseDefinition.metrics.map((metric) => ({
        ...metric,
        id: metric.id.replace('claude.', `${providerId}.`),
      })),
    });
    const extraSettings: SettingsViewState = {
      ...settingsState,
      renamableProviderIds: [providerId],
      settings: {
        ...settingsState.settings,
        providers: [
          {
            id: providerId,
            enabled: true,
            detected: true,
            expanded: false,
            useProxy: false,
            metrics: [
              {
                id: `${providerId}.session`,
                enabled: true,
                section: 'alwaysVisible',
                pinned: true,
              },
            ],
          },
        ],
      },
    };
    const extraState: ProviderViewState = {
      ...claudeState,
      snapshot: claudeState.snapshot
        ? { ...claudeState.snapshot, providerId }
        : claudeState.snapshot,
    };
    const usage: UsageViewState = { providers: { [providerId]: extraState } };
    mockInvoke((command: string, args?: InvokeArgs) => {
      if (command === 'get_usage_state') return Promise.resolve(usage);
      if (command === 'get_app_settings') return Promise.resolve(extraSettings);
      if (command === 'save_app_settings')
        return Promise.resolve({
          ...extraSettings,
          settings: args?.settings ?? extraSettings.settings,
        });
      if (command === 'get_panel_resize_edge') return Promise.resolve('bottom');
      if (command === 'get_panel_height_mode') return Promise.resolve('automatic');
      if (command === 'fit_panel_to_content') return Promise.resolve(true);
      return Promise.resolve();
    }, extraCatalog);

    render(App);
    const provider = await screen.findByRole('group', { name: 'Claude — Work provider' });
    await fireEvent.contextMenu(provider);
    await fireEvent.click(screen.getByRole('menuitem', { name: 'Rename…' }));
    const dialog = screen.getByRole('dialog', { name: 'Rename Card' });
    await fireEvent.input(within(dialog).getByRole('textbox', { name: 'Name' }), {
      target: { value: 'Client' },
    });
    await fireEvent.click(within(dialog).getByRole('button', { name: 'Rename' }));

    expect(await screen.findByRole('heading', { name: 'Client' })).toBeInTheDocument();
    expect(mocks.invoke).toHaveBeenCalledWith('save_app_settings', {
      expectedSettingsRevision: 0,
      expectedAccountRevision: 0,
      settings: expect.objectContaining({ providerNames: { [providerId]: 'Client' } }),
    });
  });

  it('persists Total Spend metric and period choices', async () => {
    render(App);
    await screen.findByText('Plus');
    await fireEvent.click(screen.getByRole('combobox', { name: 'Total Spend Metric' }));
    await fireEvent.click(screen.getByRole('option', { name: 'Tokens' }));
    await fireEvent.click(screen.getByRole('button', { name: '30 Days' }));
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith(
        'save_app_settings',
        expect.objectContaining({
          settings: expect.objectContaining({ totalSpendPeriod: 'last30Days' }),
        }),
      ),
    );
  });

  it('explains unavailable cost and reveals measured tokens for the same period', async () => {
    mockInvoke((command: string, args?: { settings?: SettingsViewState['settings'] }) => {
      if (command === 'get_usage_state')
        return Promise.resolve({
          providers: {
            codex: {
              ...codexState,
              snapshot: {
                ...codexState.snapshot!,
                usage: {
                  ...codexState.snapshot!.usage,
                  today: {
                    tokens: 2_100_000,
                    estimatedCostUsd: null,
                    costEstimated: true,
                    estimateComplete: false,
                  },
                },
              },
            },
          },
        });
      if (command === 'get_app_settings') return Promise.resolve(settingsState);
      if (command === 'save_app_settings')
        return Promise.resolve({
          ...settingsState,
          settings: args?.settings ?? settingsState.settings,
        });
      return Promise.resolve(liveState);
    });
    render(App);
    const totalSpend = await screen.findByRole('region', { name: 'Total Spend' });
    expect(within(totalSpend).getByText('No cost data for this period')).toBeInTheDocument();
    await fireEvent.click(within(totalSpend).getByRole('combobox', { name: 'Total Spend Metric' }));
    await fireEvent.click(screen.getByRole('option', { name: 'Tokens' }));
    expect(within(totalSpend).getByText('Codex')).toBeInTheDocument();
    expect(within(totalSpend).getByText('2.1')).toBeInTheDocument();
    expect(within(totalSpend).getByText('million')).toBeInTheDocument();
    expect(within(totalSpend).getByText('2.1M')).toBeInTheDocument();
    expect(within(totalSpend).queryByText('No data')).not.toBeInTheDocument();
  });

  it('reveals On Demand metrics without losing their saved order', async () => {
    render(App);
    await screen.findByText('Plus');
    expect(
      within(screen.getByRole('group', { name: 'Codex provider' })).queryByText(
        '$3.8 · 2.1M tokens',
      ),
    ).not.toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Show more' }));
    expect(screen.getByText('$3.8 · 2.1M tokens')).toBeInTheDocument();
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith('save_app_settings', expect.any(Object)),
    );
  });

  it('shows only the selected provider while Codex On Demand morphs', async () => {
    const multiUsage: UsageViewState = {
      providers: { claude: claudeState, codex: codexState },
    };
    const multiSettings: SettingsViewState = {
      ...settingsState,
      settings: {
        ...settingsState.settings,
        providers: [
          {
            id: 'claude',
            enabled: true,
            detected: true,
            expanded: false,
            useProxy: false,
            metrics: [
              { id: 'claude.session', enabled: true, section: 'alwaysVisible', pinned: true },
            ],
          },
          settingsState.settings.providers[0],
        ],
      },
    };
    mockInvoke((command: string, args?: InvokeArgs) => {
      if (command === 'get_usage_state') return Promise.resolve(multiUsage);
      if (command === 'get_app_settings') return Promise.resolve(multiSettings);
      if (command === 'save_app_settings')
        return Promise.resolve({
          ...multiSettings,
          settings: args?.settings ?? multiSettings.settings,
        });
      return Promise.resolve();
    });

    render(App);
    await screen.findByRole('group', { name: 'Claude provider' });
    await fireEvent.click(screen.getByRole('tab', { name: /Codex/ }));
    const codex = await screen.findByRole('group', { name: 'Codex provider' });
    expect(screen.queryByRole('group', { name: 'Claude provider' })).not.toBeInTheDocument();
    await fireEvent.click(within(codex).getByRole('button', { name: 'Show more' }));
    expect(within(codex).getByText('Spark')).toBeInTheDocument();
    expect(codex.closest('.provider-reorder-shell')).toHaveClass(
      'provider-reorder-shell--content-morph',
    );
  });

  it('keeps quick links visible while the compact caret controls on-demand metrics', async () => {
    render(App);
    const toggle = await screen.findByRole('button', { name: 'Show more' });
    const providerHeader = screen.getByRole('group', { name: 'Drag Codex to reorder' });
    expect(providerHeader).toHaveAttribute('data-reorder-handle');
    expect(providerHeader.closest('.provider-section')).toHaveAttribute(
      'data-reorder-group',
      'dashboard-providers',
    );
    expect(providerHeader).not.toHaveAttribute('draggable');
    expect(toggle).toHaveAttribute('aria-expanded', 'false');
    expect(toggle).not.toHaveTextContent('On Demand');
    expect(screen.getByRole('button', { name: 'Status, opens in browser' })).toBeInTheDocument();
    await fireEvent.click(toggle);
    expect(screen.getByRole('button', { name: 'Show less' })).toHaveAttribute(
      'aria-expanded',
      'true',
    );
    await fireEvent.click(screen.getByRole('button', { name: 'Status, opens in browser' }));
    expect(mocks.invoke).toHaveBeenCalledWith('open_provider_link', {
      providerId: 'codex',
      linkIndex: 0,
    });
    expect(screen.getByRole('button', { name: 'Dashboard, opens in browser' })).toBeInTheDocument();
  });

  it('shows quick links without an expander when no on-demand metrics are enabled', async () => {
    const linksOnlySettings = structuredClone(settingsState);
    linksOnlySettings.settings.providers[0].metrics =
      linksOnlySettings.settings.providers[0].metrics.map((metric) =>
        metric.section === 'onDemand' ? { ...metric, enabled: false } : metric,
      );
    mockInvoke((command: string, args?: InvokeArgs) => {
      if (command === 'get_usage_state') return Promise.resolve(liveState);
      if (command === 'get_app_settings') return Promise.resolve(linksOnlySettings);
      if (command === 'save_app_settings')
        return Promise.resolve({
          ...linksOnlySettings,
          settings: args?.settings ?? linksOnlySettings.settings,
        });
      return Promise.resolve();
    });

    render(App);
    await screen.findByText('Plus');

    expect(screen.getByRole('button', { name: 'Status, opens in browser' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Dashboard, opens in browser' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Show more' })).toBeNull();
  });

  it('renders the Total Spend ring as separated rounded SVG sectors', async () => {
    render(App);
    expect(await screen.findByRole('region', { name: 'Total Spend' })).toBeInTheDocument();
    await waitFor(() => expect(document.querySelector('.spend-ring svg')).not.toBeNull());
    const segment = document.querySelector('.spend-ring__segment');
    expect(segment?.tagName).toBe('path');
    expect(segment?.getAttribute('d')).toMatch(/^M .* A .* Q .* Z$/);
    expect(document.querySelector('.spend-ring__track')).toBeNull();
    expect(document.querySelector('.period-switcher__selection')).not.toBeNull();
  });

  it('offers Trae sign-in directly on its dashboard card', async () => {
    const fixture = webviewAuthFixture();
    mockInvoke((command: string) => {
      if (command === 'get_usage_state') return Promise.resolve(fixture.usage);
      if (command === 'get_app_settings') return Promise.resolve(fixture.settings);
      if (command === 'save_app_settings') return Promise.resolve(fixture.settings);
      if (command === 'get_provider_session_state') {
        return Promise.resolve({ providerId: 'trae-cn', status: 'notSet' });
      }
      if (command === 'open_provider_webview_login') return Promise.resolve();
      if (command === 'capture_provider_session') {
        return Promise.resolve({ providerId: 'trae-cn', status: 'saved' });
      }
      if (command === 'check_for_updates') {
        return Promise.resolve({
          available: false,
          currentVersion: '0.7.7',
          version: null,
          body: null,
          installable: true,
          releaseUrl: 'https://github.com/deviffyy/OpenQuota/releases/latest',
        });
      }
      return Promise.resolve();
    }, fixture.catalog);

    render(App);
    const provider = await screen.findByRole('group', { name: 'TraeWork CN provider' });

    const open = within(provider).getByRole('button', { name: 'Open Sign-In' });
    const capture = within(provider).getByRole('button', { name: 'I Have Signed In' });
    await fireEvent.click(open);
    await fireEvent.click(capture);

    expect(mocks.invoke).toHaveBeenCalledWith('open_provider_webview_login', {
      providerId: 'trae-cn',
    });
    expect(mocks.invoke).toHaveBeenCalledWith('capture_provider_session', {
      providerId: 'trae-cn',
    });
    expect(within(provider).getByRole('status')).toHaveTextContent('Connected');
  });

  it('offers device-code sign-in on the card of a provider whose credentials are unusable', async () => {
    const fixture = deviceCodeSignInFixture();
    mockInvoke((command: string) => {
      if (command === 'get_usage_state') return Promise.resolve(fixture.usage);
      if (command === 'get_app_settings') return Promise.resolve(fixture.settings);
      if (command === 'save_app_settings') return Promise.resolve(fixture.settings);
      if (command === 'get_provider_session_state') {
        return Promise.resolve({ providerId: 'workbuddy-cn', status: 'notSet' });
      }
      if (command === 'start_provider_login') {
        return Promise.resolve({
          loginId: 'login-1',
          verificationUri: 'https://example.test/device',
          expiresIn: 600,
        });
      }
      if (command === 'check_for_updates') {
        return Promise.resolve({
          available: false,
          currentVersion: '0.7.7',
          version: null,
          body: null,
          installable: true,
          releaseUrl: 'https://github.com/deviffyy/OpenQuota/releases/latest',
        });
      }
      return Promise.resolve();
    }, fixture.catalog);

    render(App);
    const provider = await screen.findByRole('group', { name: 'Workbuddy CN provider' });

    await fireEvent.click(within(provider).getByRole('button', { name: 'Start Sign-In' }));

    expect(mocks.invoke).toHaveBeenCalledWith('start_provider_login', {
      providerId: 'workbuddy-cn',
    });
    expect(await within(provider).findByText('https://example.test/device')).toBeInTheDocument();
  });

  it('keeps device-code sign-in off the card for an error that signing in cannot fix', async () => {
    const fixture = deviceCodeSignInFixture('network');
    mockInvoke((command: string) => {
      if (command === 'get_usage_state') return Promise.resolve(fixture.usage);
      if (command === 'get_app_settings') return Promise.resolve(fixture.settings);
      if (command === 'check_for_updates') {
        return Promise.resolve({
          available: false,
          currentVersion: '0.7.7',
          version: null,
          body: null,
          installable: true,
          releaseUrl: 'https://github.com/deviffyy/OpenQuota/releases/latest',
        });
      }
      return Promise.resolve();
    }, fixture.catalog);

    render(App);
    const provider = await screen.findByRole('group', { name: 'Workbuddy CN provider' });

    expect(
      within(provider).getByRole('button', { name: 'Retry Workbuddy CN' }),
    ).toBeInTheDocument();
    expect(within(provider).queryByRole('button', { name: 'Start Sign-In' })).toBeNull();
  });

  it('opens Customize and exposes the two-section metric layout', async () => {
    render(App);
    await screen.findByText('Plus');
    await openSettingsWorkspace('customize');
    // The detail pane names itself "Customize {provider}" and the workspace
    // column carries the provider name.
    expect(screen.getByRole('region', { name: 'Customize Codex' })).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Codex' })).toBeInTheDocument();
    expect(screen.getByRole('group', { name: 'Always Visible metrics' })).toBeInTheDocument();
    expect(screen.getByRole('group', { name: 'On Demand metrics' })).toBeInTheDocument();
  });

  it('resets one provider through the backend metric catalog', async () => {
    render(App);
    await screen.findByText('Plus');
    await openSettingsWorkspace('customize');
    await fireEvent.click(screen.getByRole('button', { name: 'Reset Codex' }));

    expect(mocks.invoke).toHaveBeenCalledWith('reset_provider_customization', {
      providerId: 'codex',
      expectedSettingsRevision: 0,
      expectedAccountRevision: 0,
    });
  });

  it('enforces the two-pinned-metrics limit in Customize', async () => {
    render(App);
    await screen.findByText('Plus');
    await openSettingsWorkspace('customize');
    await fireEvent.click(screen.getByRole('button', { name: 'Pin Today' }));
    expect(screen.getByText('Up to 2 stars per provider')).toBeInTheDocument();
  });

  it('persists Used/Left changes made directly from a quota row', async () => {
    render(App);
    await screen.findByText('Plus');
    await fireEvent.click(screen.getByRole('button', { name: '68% left' }));
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith(
        'save_app_settings',
        expect.objectContaining({ settings: expect.objectContaining({ usageDisplay: 'used' }) }),
      ),
    );
  });

  it('persists compact density from Settings', async () => {
    render(App);
    await screen.findByText('Plus');
    await openSettingsWorkspace();
    await fireEvent.click(screen.getByRole('combobox', { name: 'Density' }));
    await fireEvent.click(screen.getByRole('option', { name: 'Compact' }));
    await fireEvent.click(screen.getByRole('combobox', { name: 'Time Format' }));
    await fireEvent.click(screen.getByRole('option', { name: '24-hour' }));
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith(
        'save_app_settings',
        expect.objectContaining({ settings: expect.objectContaining({ density: 'compact' }) }),
      ),
    );
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith(
        'save_app_settings',
        expect.objectContaining({
          settings: expect.objectContaining({ timeFormat: 'twentyFourHour' }),
        }),
      ),
    );
  });

  it('persists the log level and exposes only the backend-owned log location', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: { writeText },
    });
    render(App);
    await screen.findByText('Plus');
    await openSettingsWorkspace();
    await fireEvent.click(screen.getByRole('combobox', { name: 'Log Level' }));
    await fireEvent.click(screen.getByRole('option', { name: 'Debug' }));
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith(
        'save_app_settings',
        expect.objectContaining({ settings: expect.objectContaining({ logLevel: 'debug' }) }),
      ),
    );

    await fireEvent.click(screen.getByRole('button', { name: 'Copy Log Path' }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith('C:\\Quota01\\logs\\Quota01.log'));
    expect(mocks.invoke).toHaveBeenCalledWith('get_log_path');
    expect(screen.getByRole('status')).toHaveTextContent('Log path copied');

    const headings = screen
      .getAllByRole('heading', { level: 2 })
      .map((heading) => heading.textContent?.trim());
    expect(headings.indexOf('Advanced')).toBeLessThan(headings.indexOf('Updates'));
    expect(headings).not.toContain('Data');
    expect(screen.queryByText('Application Data')).not.toBeInTheDocument();

    await fireEvent.click(
      screen.getByRole('button', {
        name: /Reveal in Finder|Reveal in File Explorer|Open Containing Folder/,
      }),
    );
    expect(mocks.invoke).toHaveBeenCalledWith('open_log_folder');
  });

  it('shows log action failures inside the Advanced card', async () => {
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: { writeText: vi.fn().mockResolvedValue(undefined) },
    });
    render(App);
    await screen.findByText('Plus');
    await openSettingsWorkspace();

    mocks.invoke.mockRejectedValueOnce(new Error('log path unavailable'));
    await fireEvent.click(screen.getByRole('button', { name: 'Copy Log Path' }));

    expect(await screen.findByRole('alert')).toHaveTextContent(
      "Couldn't copy the log path to the clipboard.",
    );
  });

  it('shows the detected Linux fallback mode in Settings', async () => {
    mocks.windowLabel = 'settings';
    mockInvoke((command: string) => {
      if (command === 'get_usage_state') return Promise.resolve(liveState);
      if (command === 'get_app_settings')
        return Promise.resolve({
          ...settingsState,
          trayAvailable: false,
          platformSummary: 'GNOME · Wayland · standalone window',
        });
      if (command === 'check_for_updates')
        return Promise.resolve({
          available: false,
          currentVersion: '0.1.0',
          version: null,
          body: null,
          installable: true,
          releaseUrl: 'https://github.com/deviffyy/OpenQuota/releases/latest',
        });
      return Promise.resolve();
    });
    render(App);
    await waitFor(() => expect(document.querySelector('[data-settings-workspace]')).toBeTruthy());
    expect(screen.queryByRole('button', { name: 'Close Quota01' })).not.toBeInTheDocument();
    expect(screen.getByText('GNOME · Wayland · standalone window')).toBeInTheDocument();
    expect(screen.queryByRole('combobox', { name: 'Window Mode' })).not.toBeInTheDocument();
  });

  it('records a global shortcut and requests notification permission', async () => {
    render(App);
    await screen.findByText('Plus');
    await openSettingsWorkspace();
    const recorder = screen.getByRole('button', { name: 'Record Shortcut' });
    await fireEvent.click(recorder);
    expect(recorder).toHaveAttribute('aria-pressed', 'true');
    await fireEvent.blur(recorder);
    expect(recorder).toHaveAttribute('aria-pressed', 'false');
    await fireEvent.click(recorder);
    expect(await fireEvent.keyDown(recorder, { key: 'Tab' })).toBe(true);
    expect(recorder).toHaveAttribute('aria-pressed', 'false');
    await fireEvent.click(recorder);
    await fireEvent.keyDown(recorder, { key: 'Q', code: 'KeyQ', ctrlKey: true, shiftKey: true });
    await fireEvent.click(screen.getByRole('checkbox', { name: /Almost Out/ }));
    await waitFor(() => {
      expect(mocks.invoke).toHaveBeenCalledWith(
        'save_app_settings',
        expect.objectContaining({
          settings: expect.objectContaining({ globalShortcut: 'Ctrl+Shift+Q' }),
        }),
      );
      expect(mocks.invoke).toHaveBeenCalledWith('request_notification_permission');
    });
    expect(screen.getByRole('checkbox', { name: /Almost Out/ })).toBeChecked();
  });

  it('confirms a full settings reset without deleting credentials or usage data', async () => {
    render(App);
    await screen.findByText('Plus');
    await openSettingsWorkspace();
    const trigger = screen.getByRole('button', { name: 'Reset All Settings…' });
    trigger.focus();
    await fireEvent.click(trigger);

    const dialog = screen.getByRole('alertdialog', { name: 'Reset All Settings?' });
    expect(dialog).toHaveTextContent(
      'Provider sign-ins, API keys, and usage history stay in place.',
    );
    expect(mocks.invoke).not.toHaveBeenCalledWith('reset_all_settings', expect.anything());
    const cancel = screen.getByRole('button', { name: 'Cancel' });
    await waitFor(() => expect(cancel).toHaveFocus());
    await fireEvent.click(cancel);
    await waitFor(() => expect(trigger).toHaveFocus());

    await fireEvent.click(trigger);
    await fireEvent.click(screen.getByRole('button', { name: 'Reset All' }));
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith('reset_all_settings', {
        expectedSettingsRevision: 0,
        expectedAccountRevision: 0,
      }),
    );
    expect(await screen.findByRole('status')).toHaveTextContent('All settings restored');
  });

  it('keeps the reset panel mode when an older mode read finishes late', async () => {
    let resolvePanelMode: ((mode: 'manual') => void) | undefined;
    mockInvoke((command: string) => {
      if (command === 'get_usage_state') return Promise.resolve(liveState);
      if (command === 'get_app_settings') return Promise.resolve(settingsState);
      if (command === 'reset_all_settings')
        return Promise.resolve({ ...settingsState, settingsRevision: 1 });
      if (command === 'get_panel_height_mode')
        return new Promise<'manual'>((resolve) => (resolvePanelMode = resolve));
      if (command === 'get_panel_resize_edge') return Promise.resolve('top');
      if (command === 'fit_panel_to_content') return Promise.resolve(true);
      if (command === 'check_for_updates')
        return Promise.resolve({
          available: false,
          currentVersion: '0.1.0',
          version: null,
          body: null,
          installable: true,
          releaseUrl: 'https://github.com/deviffyy/OpenQuota/releases/latest',
        });
      return Promise.resolve();
    });
    (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {};
    try {
      render(App);
      await screen.findByText('Plus');
      await openSettingsWorkspace();
      await fireEvent.click(screen.getByRole('button', { name: 'Reset All Settings…' }));
      await fireEvent.click(screen.getByRole('button', { name: 'Reset All' }));
      await waitFor(() =>
        expect(mocks.invoke).toHaveBeenCalledWith('reset_all_settings', {
          expectedSettingsRevision: 0,
          expectedAccountRevision: 0,
        }),
      );
      await screen.findByText('All settings restored');
      resolvePanelMode?.('manual');
      await Promise.resolve();
      expect(screen.getByRole('combobox', { name: 'Panel Height' })).toHaveTextContent('Automatic');
      await waitFor(() =>
        expect(
          mocks.invoke.mock.calls.filter(([command]) => command === 'get_panel_resize_edge').length,
        ).toBe(1),
      );
    } finally {
      delete (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
    }
  });

  it('offers system settings only when enabled notifications are blocked', async () => {
    mockInvoke((command: string, args?: { settings?: SettingsViewState['settings'] }) => {
      if (command === 'get_usage_state') return Promise.resolve(liveState);
      if (command === 'get_app_settings')
        return Promise.resolve({
          ...settingsState,
          notificationPermission: 'denied',
          settings: {
            ...settingsState.settings,
            notifications: { ...settingsState.settings.notifications, almostOut: true },
          },
        });
      if (command === 'save_app_settings')
        return Promise.resolve({
          ...settingsState,
          notificationPermission: 'denied',
          settings: args?.settings ?? settingsState.settings,
        });
      if (command === 'open_notification_settings') return Promise.resolve();
      return Promise.reject(new Error(`unexpected command ${command}`));
    });

    render(App);
    await screen.findByText('Plus');
    await openSettingsWorkspace();
    expect(screen.getByText('Notifications are blocked')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Open Settings' }));
    expect(mocks.invoke).toHaveBeenCalledWith('open_notification_settings');
  });

  it('preserves cached values and exposes a retryable stale refresh error', async () => {
    mockInvoke((command: string) => {
      if (command === 'get_usage_state')
        return Promise.resolve({
          providers: {
            codex: {
              ...codexState,
              source: 'cache',
              stale: true,
              error: 'Could not connect to Codex.',
              errorKind: 'network',
            },
          },
        });
      if (command === 'get_app_settings') return Promise.resolve(settingsState);
      if (command === 'refresh_provider_usage') return Promise.resolve(liveState);
      return Promise.resolve(liveState);
    });
    render(App);
    expect(await screen.findByRole('alert')).toHaveTextContent('Could not connect to Codex.');
    const outdated = screen.getByText((_, element) =>
      Boolean(element?.classList.contains('status-badge')),
    );
    expect(outdated).toHaveAttribute('data-tooltip', expect.stringMatching(/^Last updated/));
    expect(outdated).toHaveTextContent(/^Outdated\. Last updated/);
    const retry = screen.getByRole('button', { name: 'Retry Codex' });
    retry.focus();
    await fireEvent.click(retry);
    expect(mocks.invoke).toHaveBeenCalledWith('refresh_provider_usage', { providerId: 'codex' });
    await waitFor(() =>
      expect(screen.getByRole('group', { name: 'Codex provider' })).toHaveFocus(),
    );
  });

  it('supports manual refresh and popup close shortcuts', async () => {
    render(App);
    await screen.findByText('Plus');
    await fireEvent.click(screen.getByRole('button', { name: 'Refresh all provider usage' }));
    await waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith('refresh_usage'));
    await fireEvent.keyDown(document, { key: 'Escape' });
    expect(mocks.invoke).toHaveBeenCalledWith('dismiss_main_window');
  });

  it('lets reset details consume Escape before the popup shortcut', async () => {
    render(App);
    await screen.findByText('Plus');
    await fireEvent.click(screen.getByRole('button', { name: 'Show more' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Rate Limit Resets: 2 available' }));
    await fireEvent.click(screen.getAllByRole('button', { name: /Use reset expiring/ })[0]);

    const cancel = screen.getByRole('button', { name: 'Cancel' });
    await waitFor(() => expect(cancel).toHaveFocus());
    await fireEvent.keyDown(cancel, { key: 'Escape' });

    expect(screen.queryByText('Use this reset?')).not.toBeInTheDocument();
    const dialog = screen.getByRole('dialog', { name: 'Rate Limit Resets details' });
    expect(dialog).toBeVisible();
    const restoredUse = screen.getAllByRole('button', { name: /Use reset expiring/ })[0];
    await waitFor(() => expect(restoredUse).toHaveFocus());
    expect(mocks.invoke).not.toHaveBeenCalledWith('dismiss_main_window');

    await fireEvent.keyDown(restoredUse, { key: 'Escape' });
    expect(
      screen.queryByRole('dialog', { name: 'Rate Limit Resets details' }),
    ).not.toBeInTheDocument();
    expect(mocks.invoke).not.toHaveBeenCalledWith('dismiss_main_window');
  });

  it('refreshes only the provider selected in a context menu', async () => {
    let finishRefresh: ((state: UsageViewState) => void) | undefined;
    const refreshResult = new Promise<UsageViewState>((resolve) => (finishRefresh = resolve));
    const multiProviderState: UsageViewState = {
      providers: { codex: codexState, claude: claudeState },
      lastFullRefreshAt: new Date(Date.now() - 240_000).toISOString(),
    };
    const multiProviderSettings: SettingsViewState = {
      ...settingsState,
      settings: {
        ...settingsState.settings,
        showTotalSpend: false,
        providers: [
          ...settingsState.settings.providers,
          {
            id: 'claude',
            enabled: true,
            detected: true,
            expanded: false,
            useProxy: false,
            metrics: [
              {
                id: 'claude.session',
                enabled: true,
                section: 'alwaysVisible' as const,
                pinned: true,
              },
            ],
          },
        ],
      },
    };
    mockInvoke((command: string) => {
      if (command === 'get_usage_state') return Promise.resolve(multiProviderState);
      if (command === 'get_app_settings') return Promise.resolve(multiProviderSettings);
      if (command === 'refresh_provider_usage') return refreshResult;
      return Promise.resolve();
    });

    render(App);
    const codex = await screen.findByRole('group', { name: 'Codex provider' });
    expect(screen.queryByRole('group', { name: 'Claude provider' })).not.toBeInTheDocument();
    expect(screen.getByText('Next update in 1m')).toBeInTheDocument();
    await fireEvent.contextMenu(codex, {
      clientX: 120,
      clientY: 180,
    });
    await fireEvent.click(await screen.findByRole('menuitem', { name: 'Refresh Codex' }));

    expect(mocks.invoke).toHaveBeenCalledWith('refresh_provider_usage', { providerId: 'codex' });
    expect(within(codex).getByLabelText('Refreshing')).toBeInTheDocument();
    expect(screen.queryByRole('group', { name: 'Claude provider' })).not.toBeInTheDocument();
    expect(screen.getByText('Updating…')).toBeInTheDocument();

    finishRefresh?.(multiProviderState);
    await waitFor(() =>
      expect(within(codex).queryByLabelText('Refreshing')).not.toBeInTheDocument(),
    );
    expect(screen.getByText('Next update in 1m')).toBeInTheDocument();
  });

  it('keeps the full-refresh schedule when a provider refresh fails to start', async () => {
    const state = {
      ...liveState,
      lastFullRefreshAt: new Date(Date.now() - 240_000).toISOString(),
    };
    mockInvoke((command: string) => {
      if (command === 'get_usage_state') return Promise.resolve(state);
      if (command === 'get_app_settings') return Promise.resolve(settingsState);
      if (command === 'refresh_provider_usage') return Promise.reject(new Error('offline'));
      return Promise.resolve();
    });

    render(App);
    const provider = await screen.findByRole('group', { name: 'Codex provider' });
    expect(screen.getByText('Next update in 1m')).toBeInTheDocument();
    await fireEvent.contextMenu(provider, { clientX: 120, clientY: 180 });
    await fireEvent.click(await screen.findByRole('menuitem', { name: 'Refresh Codex' }));

    expect(await screen.findByText('Codex usage could not be refreshed.')).toBeInTheDocument();
    expect(within(provider).queryByLabelText('Refreshing')).not.toBeInTheDocument();
    expect(screen.getByText('Next update in 1m')).toBeInTheDocument();
  });

  it('keeps the Claude card structure stable while optional quota data refreshes', async () => {
    let finishRefresh: ((state: UsageViewState) => void) | undefined;
    const refreshResult = new Promise<UsageViewState>((resolve) => (finishRefresh = resolve));
    const initialClaude: ProviderViewState = {
      ...claudeState,
      snapshot: {
        ...claudeState.snapshot!,
        quotas: claudeState.snapshot!.quotas.filter((quota) => quota.id !== 'extra'),
      },
    };
    const initialState: UsageViewState = { providers: { claude: initialClaude } };
    const refreshedState: UsageViewState = { providers: { claude: claudeState } };
    const claudeSettings: SettingsViewState = {
      ...settingsState,
      settings: {
        ...settingsState.settings,
        showTotalSpend: false,
        providers: [
          {
            id: 'claude',
            enabled: true,
            detected: true,
            expanded: false,
            useProxy: false,
            metrics: [
              {
                id: 'claude.session',
                enabled: true,
                section: 'alwaysVisible',
                pinned: true,
              },
              {
                id: 'claude.extra',
                enabled: true,
                section: 'alwaysVisible',
                pinned: false,
              },
            ],
          },
        ],
      },
    };
    mockInvoke((command: string) => {
      if (command === 'get_usage_state') return Promise.resolve(initialState);
      if (command === 'get_app_settings') return Promise.resolve(claudeSettings);
      if (command === 'refresh_usage') return refreshResult;
      return Promise.resolve();
    });

    render(App);
    const provider = await screen.findByRole('group', { name: 'Claude provider' });
    const card = within(provider).getByRole('region', { name: 'Claude usage' });
    const extraRow = within(provider).getByRole('group', { name: 'Extra Usage options' });
    expect(within(extraRow).getByText('No data')).toBeInTheDocument();
    const statusSlot = provider.querySelector('.provider-status-slot');
    expect(statusSlot).toBeInTheDocument();
    expect(statusSlot).not.toHaveClass('active');

    await fireEvent.click(screen.getByRole('button', { name: 'Refresh all provider usage' }));
    expect(await within(provider).findByLabelText('Refreshing')).toBeInTheDocument();
    expect(statusSlot).toHaveClass('active');
    expect(within(provider).getByRole('region', { name: 'Claude usage' })).toBe(card);
    expect(within(provider).getByRole('group', { name: 'Extra Usage options' })).toBe(extraRow);

    finishRefresh?.(refreshedState);
    await waitFor(() => expect(within(extraRow).queryByText('No data')).not.toBeInTheDocument());
    expect(within(provider).getByRole('region', { name: 'Claude usage' })).toBe(card);
    expect(within(provider).getByRole('group', { name: 'Extra Usage options' })).toBe(extraRow);
    expect(statusSlot).not.toHaveClass('active');
  });

  it('keeps provider chrome and card alignment while initial Claude usage is loading', async () => {
    const pendingClaude: ProviderViewState = {
      source: 'none',
      refreshing: true,
      stale: false,
      error: null,
      errorKind: null,
      lastAttemptAt: null,
      snapshot: null,
    };
    const claudeSettings: SettingsViewState = {
      ...settingsState,
      settings: {
        ...settingsState.settings,
        showTotalSpend: false,
        providers: [
          {
            id: 'claude',
            enabled: true,
            detected: true,
            expanded: false,
            useProxy: false,
            metrics: [
              {
                id: 'claude.session',
                enabled: true,
                section: 'alwaysVisible',
                pinned: true,
              },
              {
                id: 'claude.weekly',
                enabled: true,
                section: 'alwaysVisible',
                pinned: true,
              },
            ],
          },
        ],
      },
    };
    mockInvoke((command: string) => {
      if (command === 'get_usage_state')
        return Promise.resolve({ providers: { claude: pendingClaude } });
      if (command === 'get_app_settings') return Promise.resolve(claudeSettings);
      return new Promise(() => undefined);
    });

    render(App);
    const provider = await screen.findByRole('group', { name: 'Claude provider' });
    const card = within(provider).getByRole('region', { name: 'Claude usage' });

    expect(within(provider).getByRole('heading', { name: 'Claude' })).toBeInTheDocument();
    expect(within(provider).getByLabelText('Refreshing')).toBeInTheDocument();
    expect(card).toHaveClass('provider-card');
    expect(card).toHaveAttribute('aria-busy', 'true');
    const session = within(card).getByRole('group', { name: 'Session options' });
    const weekly = within(card).getByRole('group', { name: 'Weekly options' });
    expect(within(session).getByText('No data')).toBeInTheDocument();
    expect(within(weekly).getByText('No data')).toBeInTheDocument();
    expect(within(card).queryByText('Reading Claude usage…')).toBeNull();
    expect(
      within(card).getByRole('button', { name: 'Status, opens in browser' }),
    ).toBeInTheDocument();
  });

  it('shows configured metric rows before a provider has produced any state', async () => {
    mockInvoke((command: string) => {
      if (command === 'get_usage_state') return Promise.resolve({ providers: {} });
      if (command === 'get_app_settings') return Promise.resolve(settingsState);
      return Promise.resolve();
    });

    render(App);
    const provider = await screen.findByRole('group', { name: 'Codex provider' });
    const card = within(provider).getByRole('region', { name: 'Codex usage' });

    expect(within(provider).queryByLabelText('Refreshing')).toBeNull();
    expect(card).not.toHaveAttribute('aria-busy');
    expect(
      within(within(card).getByRole('group', { name: 'Session options' })).getByText('No data'),
    ).toBeInTheDocument();
    expect(
      within(within(card).getByRole('group', { name: 'Weekly options' })).getByText('No data'),
    ).toBeInTheDocument();
  });

  it('keeps a snapshot-less provider error visible alongside its no-data metric rows', async () => {
    const failedCodex: ProviderViewState = {
      source: 'none',
      refreshing: false,
      stale: false,
      error: 'Sign in to Codex to load usage.',
      errorKind: 'authentication',
      lastAttemptAt: new Date().toISOString(),
      snapshot: null,
    };
    mockInvoke((command: string) => {
      if (command === 'get_usage_state')
        return Promise.resolve({ providers: { codex: failedCodex } });
      if (command === 'get_app_settings') return Promise.resolve(settingsState);
      return Promise.resolve();
    });

    render(App);
    const provider = await screen.findByRole('group', { name: 'Codex provider' });
    const card = within(provider).getByRole('region', { name: 'Codex usage' });

    expect(within(provider).getByRole('alert')).toHaveTextContent(
      'Sign in to Codex to load usage.',
    );
    expect(
      within(provider).queryByRole('button', { name: 'Configure Codex' }),
    ).not.toBeInTheDocument();
    expect(within(provider).getByRole('button', { name: 'Retry Codex' })).toBeInTheDocument();
    expect(provider.querySelector('.provider-status-slot')).toHaveClass('active');
    expect(
      within(within(card).getByRole('group', { name: 'Session options' })).getByText('No data'),
    ).toBeInTheDocument();
  });

  it('keeps local-source failures and the cached update time visible on the provider', async () => {
    const failedCodex = structuredClone(codexState);
    failedCodex.error = 'Codex credentials are unavailable. Check auth.json.';
    failedCodex.errorKind = 'credentialsUnavailable';
    failedCodex.stale = true;
    failedCodex.snapshot!.refreshedAt = new Date(Date.now() - 3 * 60 * 60 * 1000).toISOString();
    mockInvoke((command: string) => {
      if (command === 'get_usage_state')
        return Promise.resolve({ providers: { codex: failedCodex } });
      if (command === 'get_app_settings') return Promise.resolve(settingsState);
      return Promise.resolve();
    });

    render(App);

    const provider = await screen.findByRole('group', { name: 'Codex provider' });
    expect(within(provider).getByRole('heading', { name: 'Codex' })).toBeInTheDocument();
    expect(within(provider).getByRole('alert')).toHaveTextContent(
      'Codex credentials are unavailable. Check auth.json.',
    );
    expect(within(provider).getByText('Outdated')).toHaveAttribute(
      'data-tooltip',
      expect.stringMatching(/Last updated 3h/),
    );
    expect(settingsState.settings.providers.some((item) => item.id === 'codex')).toBe(true);
  });

  it('offers configuration when an API-key provider needs authentication', async () => {
    const definition = providerCatalog.providers.find((provider) => provider.id === 'openrouter')!;
    const failedOpenRouter: ProviderViewState = {
      source: 'none',
      refreshing: false,
      stale: false,
      error: 'Add an OpenRouter API key in Customize to view usage.',
      errorKind: 'authentication',
      lastAttemptAt: new Date().toISOString(),
      snapshot: null,
    };
    mockInvoke((command: string) => {
      if (command === 'get_usage_state')
        return Promise.resolve({ providers: { openrouter: failedOpenRouter } });
      if (command === 'get_app_settings')
        return Promise.resolve({
          ...settingsState,
          settings: {
            ...settingsState.settings,
            providers: [
              {
                id: 'openrouter',
                enabled: true,
                detected: false,
                expanded: false,
                useProxy: false,
                metrics: definition.metrics.map((metric) => ({
                  id: metric.id,
                  enabled: metric.defaultEnabled,
                  section: metric.defaultSection,
                  pinned: metric.defaultPinned,
                })),
              },
            ],
          },
        });
      if (command === 'get_provider_api_key_state')
        return Promise.resolve({ providerId: 'openrouter', status: 'notSet' });
      return Promise.resolve();
    });

    render(App);
    const provider = await screen.findByRole('group', { name: 'OpenRouter provider' });
    await fireEvent.click(within(provider).getByRole('button', { name: 'Configure OpenRouter' }));
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith('open_settings_window', {
        target: 'provider:openrouter',
      }),
    );
    expect(screen.getByRole('group', { name: 'OpenRouter provider' })).toBeInTheDocument();
    await mountSettingsWindow('provider:openrouter');
    expect(await screen.findByRole('region', { name: 'OpenRouter API Key' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Back' })).not.toBeInTheDocument();
  });

  it('opens provider settings in a separate window without routing the popup', async () => {
    render(App);
    const provider = await screen.findByRole('group', { name: 'Codex provider' });
    const settingsButton = within(provider).getByRole('button', { name: 'Settings for Codex' });

    await fireEvent.pointerDown(settingsButton);
    await fireEvent.click(settingsButton);

    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith('open_settings_window', {
        target: 'provider:codex',
      }),
    );
    expect(screen.getByRole('group', { name: 'Codex provider' })).toBeInTheDocument();
    expect(screen.queryByRole('region', { name: 'Customize Codex' })).not.toBeInTheDocument();
    expect(mocks.invoke).not.toHaveBeenCalledWith('set_panel_layout_for_screen', {
      screen: 'settings',
    });
  });

  it('opens Settings in a separate window and leaves the popup data view unchanged', async () => {
    const multiUsage: UsageViewState = {
      providers: { claude: claudeState, codex: codexState },
    };
    const multiSettings: SettingsViewState = {
      ...settingsState,
      settings: {
        ...settingsState.settings,
        providers: [
          {
            id: 'claude',
            enabled: true,
            detected: true,
            expanded: false,
            useProxy: false,
            metrics: [
              { id: 'claude.session', enabled: true, section: 'alwaysVisible', pinned: true },
            ],
          },
          ...settingsState.settings.providers,
        ],
      },
    };
    mockInvoke((command) => {
      if (
        command === 'get_usage_state' ||
        command === 'refresh_usage' ||
        command === 'refresh_provider_usage'
      )
        return Promise.resolve(multiUsage);
      if (command === 'get_app_settings') return Promise.resolve(multiSettings);
      if (command === 'check_for_updates')
        return Promise.resolve({
          available: false,
          currentVersion: '0.1.0',
          version: null,
          body: null,
          installable: true,
          releaseUrl: 'https://github.com/deviffyy/OpenQuota/releases/latest',
        });
      if (command === 'fit_panel_to_content') return Promise.resolve(true);
      return Promise.resolve();
    });

    render(App);
    await screen.findByRole('group', { name: 'Claude provider' });
    await fireEvent.click(screen.getByRole('button', { name: 'Open Settings' }));

    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith('open_settings_window', { target: 'settings' }),
    );
    expect(screen.getByRole('group', { name: 'Claude provider' })).toBeInTheDocument();
    expect(screen.queryByRole('region', { name: 'Settings' })).not.toBeInTheDocument();
    expect(mocks.invoke).not.toHaveBeenCalledWith('set_panel_layout_for_screen', {
      screen: 'settings',
    });

    const taskbandOpen = appEventHandlers.get('taskband-open');
    expect(taskbandOpen).toBeDefined();
    taskbandOpen?.({ payload: 'codex' });
    await waitFor(() => {
      expect(screen.getByRole('tab', { name: /Codex/ })).toHaveAttribute('aria-selected', 'true');
      expect(screen.getByRole('group', { name: 'Codex provider' })).toBeInTheDocument();
      expect(document.querySelector('.screen-page[data-screen="dashboard"]')).toBeInTheDocument();
      expect(document.querySelector('[data-settings-workspace]')).toBeNull();
    });
  });

  it('renders the Settings workspace only in the dedicated settings window', async () => {
    mocks.windowLabel = 'settings';
    render(App);

    await waitFor(() => expect(document.querySelector('[data-settings-workspace]')).toBeTruthy());
    expect(screen.queryByRole('group', { name: 'Codex provider' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Back' })).not.toBeInTheDocument();
    expect(mocks.invoke).not.toHaveBeenCalledWith('set_panel_layout_for_screen', {
      screen: 'settings',
    });
  });

  it('restores stable provider chrome when a refresh request fails to start', async () => {
    const state = {
      ...liveState,
      lastFullRefreshAt: new Date(Date.now() - 240_000).toISOString(),
    };
    mockInvoke((command: string) => {
      if (command === 'get_usage_state') return Promise.resolve(state);
      if (command === 'get_app_settings') return Promise.resolve(settingsState);
      if (command === 'refresh_usage') return Promise.reject(new Error('offline'));
      return Promise.resolve();
    });
    render(App);
    const provider = await screen.findByRole('group', { name: 'Codex provider' });
    const card = within(provider).getByRole('region', { name: 'Codex usage' });
    expect(screen.getByText('Next update in 1m')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Refresh all provider usage' }));
    await waitFor(() =>
      expect(within(provider).queryByLabelText('Refreshing')).not.toBeInTheDocument(),
    );
    expect(within(provider).getByRole('region', { name: 'Codex usage' })).toBe(card);
    expect(provider.querySelector('.provider-status-slot')).not.toHaveClass('active');
    expect(screen.getByText('Quota01 could not start a provider refresh.')).toBeInTheDocument();
    expect(screen.getByText('Next update in 1m')).toBeInTheDocument();
  });

  it('shows platform-correct Ctrl shortcuts and handles Ctrl+Q', async () => {
    render(App);
    await screen.findByText('Plus');
    await fireEvent.keyDown(document, { key: ',', ctrlKey: true });
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith('open_settings_window', { target: 'settings' }),
    );

    await fireEvent.keyDown(document, { key: 'q', ctrlKey: true });
    expect(mocks.invoke).toHaveBeenCalledWith('quit_app');
  });

  it('honors Reduce Motion without overriding a manually sized native panel', async () => {
    const originalMatchMedia = window.matchMedia;
    const defaultInvoke = mocks.invoke.getMockImplementation()!;
    mocks.invoke.mockImplementation((command: string, args?: InvokeArgs) => {
      if (command === 'get_panel_height_mode') return Promise.resolve('manual');
      return defaultInvoke(command, args);
    });
    window.matchMedia = vi.fn().mockReturnValue({
      matches: true,
      media: '(prefers-reduced-motion: reduce)',
      onchange: null,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      addListener: vi.fn(),
      removeListener: vi.fn(),
      dispatchEvent: vi.fn(),
    });
    Object.defineProperty(window, '__TAURI_INTERNALS__', {
      configurable: true,
      value: {},
    });
    try {
      render(App);
      await waitFor(() => expect(document.documentElement).toHaveAttribute('data-reduced-motion'));
      await screen.findByText('Plus');
      await openSettingsWorkspace();
      await waitFor(() =>
        expect(screen.getByRole('combobox', { name: 'Panel Height' })).toHaveTextContent('Manual'),
      );
      await fireEvent.keyDown(document, { key: 'Escape' });
      await waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith('dismiss_settings_window'));
      expect(
        screen.queryByRole('separator', { name: 'Resize panel height' }),
      ).not.toBeInTheDocument();
      expect(mocks.invoke).not.toHaveBeenCalledWith('fit_panel_to_content', expect.anything());
    } finally {
      delete (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
      window.matchMedia = originalMatchMedia;
    }
  });

  it('lets the app preference reduce animations independently of the system setting', async () => {
    const originalMatchMedia = window.matchMedia;
    window.matchMedia = vi.fn().mockReturnValue({
      matches: false,
      media: '(prefers-reduced-motion: reduce)',
      onchange: null,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      addListener: vi.fn(),
      removeListener: vi.fn(),
      dispatchEvent: vi.fn(),
    });
    mockInvoke((command: string, args?: InvokeArgs) => {
      if (command === 'get_usage_state') return Promise.resolve(liveState);
      if (command === 'get_app_settings')
        return Promise.resolve({
          ...settingsState,
          settings: { ...settingsState.settings, reduceAnimations: true },
        });
      if (command === 'save_app_settings')
        return Promise.resolve({
          ...settingsState,
          settingsRevision: 1,
          settings: args?.settings ?? settingsState.settings,
        });
      if (command === 'get_panel_resize_edge') return Promise.resolve('bottom');
      if (command === 'get_panel_height_mode') return Promise.resolve('automatic');
      if (command === 'fit_panel_to_content') return Promise.resolve(true);
      return Promise.resolve();
    });
    try {
      render(App);
      await waitFor(() => expect(document.documentElement).toHaveAttribute('data-reduced-motion'));
      await screen.findByText('Plus');
      await openSettingsWorkspace();
      const toggle = screen.getByRole('checkbox', { name: 'Reduce Animations' });
      expect(toggle).toBeChecked();
      await fireEvent.click(toggle);
      await waitFor(() =>
        expect(mocks.invoke).toHaveBeenCalledWith(
          'save_app_settings',
          expect.objectContaining({
            settings: expect.objectContaining({ reduceAnimations: false }),
          }),
        ),
      );
      await waitFor(() =>
        expect(document.documentElement).not.toHaveAttribute('data-reduced-motion'),
      );
    } finally {
      window.matchMedia = originalMatchMedia;
    }
  });

  it('honors the system motion preference even when bootstrap settings fail', async () => {
    const originalMatchMedia = window.matchMedia;
    window.matchMedia = vi.fn().mockReturnValue({
      matches: true,
      media: '(prefers-reduced-motion: reduce)',
      onchange: null,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      addListener: vi.fn(),
      removeListener: vi.fn(),
      dispatchEvent: vi.fn(),
    });
    mockInvoke((command: string) => {
      if (command === 'get_usage_state') return Promise.resolve(liveState);
      if (command === 'get_app_settings') return Promise.reject(new Error('settings unavailable'));
      return Promise.resolve();
    });
    try {
      render(App);
      await waitFor(() => expect(document.documentElement).toHaveAttribute('data-reduced-motion'));
    } finally {
      window.matchMedia = originalMatchMedia;
    }
  });

  it('suppresses the WebView context menu outside custom menu targets', async () => {
    render(App);
    await screen.findByText('Plus');
    const event = new MouseEvent('contextmenu', { bubbles: true, cancelable: true });
    screen.getByLabelText('Quota01 usage dashboard').dispatchEvent(event);
    expect(event.defaultPrevented).toBe(true);
  });

  it('keeps native persistence active through synthetic pointer handoff events', async () => {
    Object.defineProperty(window, '__TAURI_INTERNALS__', {
      configurable: true,
      value: {},
    });
    try {
      render(App);
      const grip = await screen.findByRole('separator', { name: 'Resize panel height' });
      await waitFor(() => expect(grip).toHaveClass('panel-resize-dragger--bottom'));

      await fireEvent.pointerDown(grip, { button: 0 });
      expect(mocks.invoke).toHaveBeenCalledWith('begin_panel_resize');
      await waitFor(() => expect(mocks.startResizeDragging).toHaveBeenCalledWith('South'));
      await waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith('lock_panel_resize_axis'));
      expect(mocks.invoke).not.toHaveBeenCalledWith('finish_panel_resize');

      await fireEvent.pointerCancel(window);
      await fireEvent.pointerUp(window);
      expect(mocks.invoke).not.toHaveBeenCalledWith('finish_panel_resize');
    } finally {
      delete (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
    }
  });

  it('changes panel height mode from the separate Settings window', async () => {
    Object.defineProperty(window, '__TAURI_INTERNALS__', {
      configurable: true,
      value: {},
    });
    const defaultInvoke = mocks.invoke.getMockImplementation()!;
    let persistedHeightMode: PanelHeightMode = 'manual';
    mocks.invoke.mockImplementation((command: string, args?: InvokeArgs) => {
      if (command === 'get_panel_height_mode') return Promise.resolve(persistedHeightMode);
      if (command === 'set_panel_height_automatic') {
        persistedHeightMode = 'automatic';
        return Promise.resolve();
      }
      if (command === 'set_panel_height_manual') {
        persistedHeightMode = 'manual';
        return Promise.resolve();
      }
      return defaultInvoke(command, args);
    });
    try {
      render(App);
      await screen.findByText('Plus');
      await openSettingsWorkspace();
      const windowMode = screen.getByRole('combobox', { name: 'Window Mode' });
      await fireEvent.click(windowMode);
      await fireEvent.click(screen.getByRole('option', { name: 'Floating Window' }));
      await waitFor(() =>
        expect(mocks.invoke).toHaveBeenCalledWith(
          'save_app_settings',
          expect.objectContaining({
            settings: expect.objectContaining({ windowMode: 'floating' }),
          }),
        ),
      );

      const heightMode = screen.getByRole('combobox', { name: 'Panel Height' });
      await waitFor(() => expect(heightMode).toHaveTextContent('Manual'));

      await fireEvent.click(heightMode);
      await fireEvent.click(screen.getByRole('option', { name: 'Automatic' }));
      expect(mocks.invoke).toHaveBeenCalledWith('set_panel_height_automatic');
      await waitFor(() => expect(heightMode).toHaveTextContent('Automatic'));

      await fireEvent.click(heightMode);
      await fireEvent.click(screen.getByRole('option', { name: 'Manual' }));
      expect(mocks.invoke).toHaveBeenCalledWith('set_panel_height_manual');
      await waitFor(() => expect(heightMode).toHaveTextContent('Manual'));

      await fireEvent.keyDown(document, { key: 'Escape' });
      await waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith('dismiss_settings_window'));
      expect(
        screen.queryByRole('separator', { name: 'Resize panel height' }),
      ).not.toBeInTheDocument();
    } finally {
      delete (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
    }
  });

  it('opens provider customization separately and keeps Settings navigation inside its window', async () => {
    appEventHandlers.clear();
    mocks.listen.mockImplementation((event, callback) => {
      appEventHandlers.set(event, callback);
      return Promise.resolve(vi.fn());
    });
    render(App);
    await screen.findByText('Plus');
    await fireEvent.contextMenu(screen.getByRole('group', { name: 'Codex provider' }), {
      clientX: 120,
      clientY: 180,
    });
    expect(screen.getByRole('menuitem', { name: 'Share Screenshot' })).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('menuitem', { name: 'Customize…' }));
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith('open_settings_window', {
        target: 'provider:codex',
      }),
    );
    expect(screen.getByRole('group', { name: 'Codex provider' })).toBeInTheDocument();

    cleanup();
    mocks.windowLabel = 'settings';
    appEventHandlers.clear();
    render(App);
    await waitFor(() => expect(document.querySelector('[data-settings-workspace]')).toBeTruthy());
    await waitFor(() => expect(appEventHandlers.get('settings-workspace-selection')).toBeDefined());
    appEventHandlers.get('settings-workspace-selection')?.({ payload: 'provider:codex' });
    expect(await screen.findByRole('region', { name: 'Customize Codex' })).toBeInTheDocument();
    appEventHandlers.get('settings-workspace-selection')?.({ payload: 'settings' });
    expect(await screen.findByRole('combobox', { name: 'Theme' })).toBeInTheDocument();
    appEventHandlers.get('settings-workspace-selection')?.({ payload: 'customize' });
    expect(await screen.findByRole('region', { name: 'Customize Codex' })).toBeInTheDocument();
    expect(document.querySelector('.screen-page[data-screen="settings"]')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Back' })).not.toBeInTheDocument();
  });

  it('supports native-like keyboard navigation in dashboard context menus', async () => {
    render(App);
    await screen.findByText('Plus');
    await fireEvent.contextMenu(screen.getByRole('group', { name: 'Codex provider' }), {
      clientX: 120,
      clientY: 180,
    });
    const hide = screen.getByRole('menuitem', { name: 'Hide Codex' });
    await waitFor(() => expect(hide).toHaveFocus());
    await fireEvent.keyDown(hide, { key: 'ArrowDown' });
    expect(screen.getByRole('menuitem', { name: 'Refresh Codex' })).toHaveFocus();
    await fireEvent.keyDown(document.activeElement!, { key: 'Escape' });
    expect(document.querySelector('.context-menu')).toBeNull();
  });

  it('does not preselect a context-menu item after a pointer invocation', async () => {
    render(App);
    await screen.findByText('Plus');
    await fireEvent.contextMenu(screen.getByRole('group', { name: 'Codex provider' }), {
      button: 2,
      clientX: 120,
      clientY: 180,
    });
    const hide = screen.getByRole('menuitem', { name: 'Hide Codex' });
    const menu = hide.closest<HTMLElement>('[role="menu"]');
    if (!menu) throw new Error('Context menu was not rendered.');
    await waitFor(() => expect(menu).toHaveFocus());
    expect(hide).not.toHaveFocus();

    await fireEvent.keyDown(menu, { key: 'ArrowDown' });
    expect(hide).toHaveFocus();
  });

  it('hides a dashboard metric without removing its menu bar star', async () => {
    render(App);
    await screen.findByText('Plus');
    await fireEvent.contextMenu(screen.getByRole('group', { name: 'Session options' }), {
      clientX: 120,
      clientY: 180,
    });
    await fireEvent.click(screen.getByRole('menuitem', { name: 'Hide' }));

    await waitFor(() => {
      const save = [...mocks.invoke.mock.calls]
        .reverse()
        .find((call: unknown[]) => call[0] === 'save_app_settings');
      const settings = save?.[1]?.settings as AppSettings | undefined;
      const session = settings?.providers
        .find((provider) => provider.id === 'codex')
        ?.metrics.find((metric) => metric.id === 'codex.session');
      expect(session).toMatchObject({ enabled: false, pinned: true });
    });
  });

  it('lets a dropdown consume Escape without navigating away from Settings', async () => {
    render(App);
    await screen.findByText('Plus');
    await openSettingsWorkspace();
    const theme = screen.getByRole('combobox', { name: 'Theme' });

    await fireEvent.keyDown(theme, { key: 'ArrowDown' });
    expect(screen.getByRole('listbox', { name: 'Theme' })).toBeInTheDocument();
    await fireEvent.keyDown(document.activeElement!, { key: 'Escape' });

    expect(screen.queryByRole('listbox', { name: 'Theme' })).not.toBeInTheDocument();
    expect(theme).toHaveFocus();
    expect(screen.getByRole('heading', { name: 'Settings', level: 1 })).toBeInTheDocument();
  });
});

describe('Quota01 Windows taskband focus', () => {
  const eventHandlers = new Map<string, (payload: unknown) => void>();

  beforeEach(() => {
    mocks.windowLabel = 'main';
    mocks.currentMonitor.mockResolvedValue({
      scaleFactor: 1,
      workArea: { size: { width: 1280, height: 700 } },
    });
    mocks.listen
      .mockReset()
      .mockImplementation((event: string, handler: (payload: unknown) => void) => {
        eventHandlers.set(event, handler);
        return Promise.resolve(vi.fn());
      });
    mocks.startDragging.mockReset().mockResolvedValue(undefined);
    mocks.startResizeDragging.mockReset().mockResolvedValue(undefined);
    eventHandlers.clear();
    mocks.invoke.mockReset();
  });
  afterEach(() => {
    cleanup();
    localStorage.clear();
  });

  it('hides other agents and keeps only the clicked agent when a taskband item is opened', async () => {
    const multiUsage: UsageViewState = {
      providers: { claude: claudeState, codex: codexState },
    };
    const multiSettings: SettingsViewState = {
      ...settingsState,
      settings: {
        ...settingsState.settings,
        providers: [
          {
            id: 'claude',
            enabled: true,
            detected: true,
            expanded: false,
            useProxy: false,
            metrics: [
              { id: 'claude.session', enabled: true, section: 'alwaysVisible', pinned: true },
            ],
          },
          settingsState.settings.providers[0],
        ],
      },
    };
    mockInvoke((command: string) => {
      if (
        command === 'get_usage_state' ||
        command === 'refresh_usage' ||
        command === 'refresh_provider_usage'
      )
        return Promise.resolve(multiUsage);
      if (command === 'get_app_settings') return Promise.resolve(multiSettings);
      if (command === 'get_panel_height_mode') return Promise.resolve('automatic');
      if (command === 'check_for_updates')
        return Promise.resolve({
          available: false,
          currentVersion: '0.1.0',
          version: null,
          body: null,
          installable: true,
          releaseUrl: 'https://github.com/deviffyy/OpenQuota/releases/latest',
        });
      if (command === 'fit_panel_to_content') return Promise.resolve(true);
      if (command === 'dismiss_main_window') return Promise.resolve();
      return Promise.resolve();
    });

    Object.defineProperty(window, '__TAURI_INTERNALS__', {
      configurable: true,
      value: {},
    });
    try {
      render(App);
      await screen.findByRole('group', { name: 'Claude provider' });
      expect(screen.queryByRole('group', { name: 'Codex provider' })).not.toBeInTheDocument();
      const fitCallsBeforeOpen = mocks.invoke.mock.calls.filter(
        ([command]) => command === 'fit_panel_to_content',
      ).length;

      const openTaskband = eventHandlers.get('taskband-open');
      expect(openTaskband).toBeDefined();
      openTaskband?.({ payload: 'codex' });

      await waitFor(() => {
        expect(screen.queryByRole('group', { name: 'Claude provider' })).not.toBeInTheDocument();
      });
      const codex = screen.getByRole('group', { name: 'Codex provider' });
      expect(codex).toBeInTheDocument();
      expect(screen.getByRole('tab', { name: /Codex/ })).toHaveAttribute('aria-selected', 'true');
      expect(within(codex).getByRole('button', { name: 'Show more' })).toBeInTheDocument();
      expect(within(codex).queryByRole('button', { name: 'Show less' })).not.toBeInTheDocument();
      expect(within(codex).queryByText('Spark')).not.toBeInTheDocument();
      expect(
        within(codex).getByRole('button', { name: 'Status, opens in browser' }),
      ).toBeInTheDocument();
      expect(screen.getByRole('region', { name: 'Total Spend' })).toBeInTheDocument();
      expect(mocks.invoke).not.toHaveBeenCalledWith('save_app_settings', expect.anything());
      await waitFor(() =>
        expect(
          mocks.invoke.mock.calls.filter(([command]) => command === 'fit_panel_to_content').length,
        ).toBeGreaterThan(fitCallsBeforeOpen),
      );

      const hideWindow = eventHandlers.get('main-window-hidden');
      expect(hideWindow).toBeDefined();
      hideWindow?.({ payload: undefined });
      await waitFor(() => {
        const globalCodex = screen.getByRole('group', { name: 'Codex provider' });
        const toggle = within(globalCodex).getByRole('button', { name: 'Show more' });
        expect(toggle).toHaveAttribute('aria-expanded', 'false');
      });
    } finally {
      delete (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
    }
  });

  it('keeps provider settings inside the single Settings page without a Back button', async () => {
    mocks.windowLabel = 'settings';
    mockInvoke((command: string) => {
      if (
        command === 'get_usage_state' ||
        command === 'refresh_usage' ||
        command === 'refresh_provider_usage'
      )
        return Promise.resolve(liveState);
      if (command === 'get_app_settings') return Promise.resolve(settingsState);
      if (command === 'check_for_updates')
        return Promise.resolve({
          available: false,
          currentVersion: '0.1.0',
          version: null,
          body: null,
          installable: true,
          releaseUrl: 'https://github.com/deviffyy/OpenQuota/releases/latest',
        });
      if (command === 'fit_panel_to_content') return Promise.resolve(true);
      if (command === 'dismiss_main_window') return Promise.resolve();
      return Promise.resolve();
    });

    render(App);
    await waitFor(() => expect(document.querySelector('[data-settings-workspace]')).toBeTruthy());

    const selectWorkspace = eventHandlers.get('settings-workspace-selection');
    expect(selectWorkspace).toBeDefined();
    selectWorkspace?.({ payload: 'provider:codex' });

    await waitFor(() => {
      expect(screen.getByRole('group', { name: 'Always Visible metrics' })).toBeInTheDocument();
    });
    expect(document.querySelector('.screen-page[data-screen="settings"]')).toBeInTheDocument();
    expect(document.querySelector('.screen-page[data-screen="provider:codex"]')).toBeNull();
    expect(screen.queryByRole('button', { name: 'Back' })).not.toBeInTheDocument();
  });
});
