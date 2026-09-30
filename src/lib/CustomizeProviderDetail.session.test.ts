import { cleanup, fireEvent, render, screen, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi, type Mock } from 'vitest';
import { addMessages, locale } from 'svelte-i18n';
import CustomizeProviderDetail from './CustomizeProviderDetail.svelte';
import { ProviderCatalogIndex } from './metrics';
import type { AppSettings, ProviderCatalog } from './types';
import { t } from './i18n';

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }));
vi.mock('@tauri-apps/api/event', () => ({ listen: mocks.listen }));

const catalogData: ProviderCatalog = {
  webviewAuthProviderIds: ['trae-cn', 'deepseek'],
  deviceCodeSignInProviderIds: ['workbuddy-cn'],
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
  schemaVersion: 10,
  proxyUrl: null,
  providers: [
    {
      id: 'trae-cn',
      enabled: false,
      detected: false,
      expanded: false,
      useProxy: false,
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
      useProxy: false,
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
      useProxy: true,
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
  beforeEach(async () => {
    await locale.set('en');
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

  it('renders the device-code sign-in panel when the provider declares that capability', async () => {
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

    expect(
      await screen.findByRole('region', { name: 'Workbuddy CN Connection' }),
    ).toBeInTheDocument();
    expect(await screen.findByRole('button', { name: 'Start Sign-In' })).toBeInTheDocument();
    expect(mocks.invoke).toHaveBeenCalledWith('get_provider_session_state', {
      providerId: 'workbuddy-cn',
    });
    expect(mocks.invoke).not.toHaveBeenCalledWith('get_provider_api_key_state', {
      providerId: 'workbuddy-cn',
    });
  });

  it('toggles only the selected provider proxy setting', async () => {
    const onChange = vi.fn();
    render(CustomizeProviderDetail, {
      settings,
      providerId: 'deepseek',
      catalog: new ProviderCatalogIndex(catalogData),
      renamableProviderIds: [],
      onChange,
      onNameChange: () => {},
      onReorderStart: () => {},
      onReorderEnd: () => {},
      reducedMotion: true,
    });

    await fireEvent.click(screen.getByRole('checkbox', { name: t('customize.useProxy') }));
    expect(screen.getByText(t('customize.useProxyHelp'))).toBeInTheDocument();

    expect(onChange).toHaveBeenLastCalledWith(
      expect.objectContaining({
        providers: expect.arrayContaining([
          expect.objectContaining({
            id: 'trae-cn',
            enabled: false,
            detected: false,
            expanded: false,
            useProxy: false,
            metrics: [
              { id: 'trae-cn.credits', enabled: true, section: 'alwaysVisible', pinned: true },
              { id: 'trae-cn.status', enabled: true, section: 'onDemand', pinned: false },
            ],
          }),
          expect.objectContaining({
            id: 'deepseek',
            enabled: false,
            detected: false,
            expanded: false,
            useProxy: true,
            metrics: [
              { id: 'deepseek.balance', enabled: true, section: 'alwaysVisible', pinned: true },
            ],
          }),
          expect.objectContaining({
            id: 'workbuddy-cn',
            enabled: false,
            detected: false,
            expanded: false,
            useProxy: true,
            metrics: [
              { id: 'workbuddy-cn.quota', enabled: true, section: 'alwaysVisible', pinned: true },
            ],
          }),
        ]),
      }),
    );
  });

  it('uses the active locale for the provider proxy label and helper text', async () => {
    addMessages('task-7-proxy-customize', {
      customize: {
        useProxy: 'Route this provider through the proxy',
        useProxyHelp: 'This localized control uses the shared URL.',
      },
    });
    await locale.set('task-7-proxy-customize');
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

    expect(
      screen.getByRole('checkbox', { name: 'Route this provider through the proxy' }),
    ).toBeInTheDocument();
    expect(screen.getByText('This localized control uses the shared URL.')).toBeInTheDocument();
  });

  it('lets macOS hide the native instance without disabling its provider', async () => {
    vi.spyOn(window.navigator, 'userAgent', 'get').mockReturnValue('Mozilla/5.0 (Macintosh)');
    const onChange = vi.fn();
    const enabledSettings = {
      ...settings,
      providers: settings.providers.map((provider) =>
        provider.id === 'deepseek' ? { ...provider, enabled: true } : provider,
      ),
    };
    render(CustomizeProviderDetail, {
      settings: enabledSettings,
      providerId: 'deepseek',
      catalog: new ProviderCatalogIndex(catalogData),
      renamableProviderIds: [],
      onChange,
      onNameChange: () => {},
      onReorderStart: () => {},
      onReorderEnd: () => {},
      reducedMotion: true,
    });

    const checkbox = screen.getByRole('checkbox', { name: 'Show in Menu Bar' });
    await fireEvent.click(checkbox);

    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({
        providers: expect.arrayContaining([
          expect.objectContaining({ id: 'deepseek', enabled: true }),
        ]),
        taskbandProviders: expect.objectContaining({
          deepseek: expect.objectContaining({ enabled: false }),
        }),
      }),
    );
  });
});

/** Controls removed from the per-provider native-instance block on BOTH platforms. */
const removedInstanceStyleLabels = [
  'customize.position',
  'customize.firstLine',
  'customize.secondLine',
  'customize.padding',
  'customize.restoreDefaults',
] as const;

/**
 * The metric list also renders `role="group"` blocks and checkboxes, the proxy
 * row reuses the `taskband-section` class, and both `settings.taskbar` and
 * `customize.taskbar` resolve to the same English string — so identifying the
 * native-instance block by accessible name is ambiguous either way. Select it
 * structurally: it is the `taskband-section` that is NOT the proxy section.
 */
function instanceBlock(): HTMLElement {
  const blocks = document.querySelectorAll<HTMLElement>(
    '.customize-detail .taskband-section:not(.proxy-setting-section)',
  );
  if (blocks.length !== 1) {
    throw new Error(`expected exactly one native-instance block, found ${blocks.length}`);
  }
  return blocks[0];
}

function instanceEnableSwitch() {
  return within(instanceBlock()).getByRole('checkbox');
}

function renderProviderDetail({
  providerId = 'deepseek',
  onChange = vi.fn(),
  overrides = {},
}: {
  providerId?: string;
  onChange?: Mock<(settings: AppSettings) => void>;
  overrides?: Partial<AppSettings>;
} = {}) {
  render(CustomizeProviderDetail, {
    settings: { ...settings, ...overrides },
    providerId,
    catalog: new ProviderCatalogIndex(catalogData),
    renamableProviderIds: [],
    onChange,
    onNameChange: () => {},
    onReorderStart: () => {},
    onReorderEnd: () => {},
    reducedMotion: true,
  });
  return { onChange };
}

describe('CustomizeProviderDetail native-instance block is toggle-only', () => {
  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it('renders only the enable switch on macOS and none of the style controls', () => {
    vi.spyOn(window.navigator, 'userAgent', 'get').mockReturnValue('Mozilla/5.0 (Macintosh)');
    renderProviderDetail();

    const block = instanceBlock();
    const switches = within(block).getAllByRole('checkbox');
    expect(switches).toHaveLength(1);
    expect(switches[0]).toHaveAccessibleName(t('customize.showOnMenuBar'));
    for (const key of removedInstanceStyleLabels) {
      expect(within(block).queryByText(t(key))).not.toBeInTheDocument();
    }
    expect(within(block).queryByRole('combobox')).not.toBeInTheDocument();
    expect(within(block).queryByRole('spinbutton')).not.toBeInTheDocument();
  });

  it('renders only the enable switch on Windows and none of the style controls', () => {
    vi.spyOn(window.navigator, 'userAgent', 'get').mockReturnValue('Mozilla/5.0 (Windows NT 10.0)');
    renderProviderDetail();

    const block = instanceBlock();
    const switches = within(block).getAllByRole('checkbox');
    expect(switches).toHaveLength(1);
    expect(switches[0]).toHaveAccessibleName(t('customize.showOnTaskbar'));
    for (const key of removedInstanceStyleLabels) {
      expect(within(block).queryByText(t(key))).not.toBeInTheDocument();
    }
    expect(within(block).queryByRole('combobox')).not.toBeInTheDocument();
    expect(within(block).queryByRole('spinbutton')).not.toBeInTheDocument();
  });

  it('preserves persisted style values when the toggle is flipped', async () => {
    vi.spyOn(window.navigator, 'userAgent', 'get').mockReturnValue('Mozilla/5.0 (Macintosh)');
    const { onChange } = renderProviderDetail({
      overrides: {
        taskbandProviders: {
          deepseek: {
            enabled: true,
            side: 'left',
            topColor: { type: 'solid', value: '#abcdef' },
            bottomColor: null,
            topBold: true,
            bottomBold: false,
            topSize: 11,
            bottomSize: 12,
            topAlign: 2,
            bottomAlign: 1,
            paddingLeft: 7,
            paddingRight: 8,
          },
        },
      },
    });

    await fireEvent.click(instanceEnableSwitch());

    // Hiding the controls must not drop the persisted styling: only `enabled` changes.
    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({
        taskbandProviders: expect.objectContaining({
          deepseek: {
            enabled: false,
            side: 'left',
            topColor: { type: 'solid', value: '#abcdef' },
            bottomColor: null,
            topBold: true,
            bottomBold: false,
            topSize: 11,
            bottomSize: 12,
            topAlign: 2,
            bottomAlign: 1,
            paddingLeft: 7,
            paddingRight: 8,
          },
        }),
      }),
    );
  });
});
