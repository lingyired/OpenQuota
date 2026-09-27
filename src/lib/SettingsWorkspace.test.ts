import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import SettingsWorkspace from './SettingsWorkspace.svelte';
import type { DashboardProps } from './Dashboard.svelte';
import type { SettingsViewState, UsageViewState } from './types';
import { ProviderCatalogIndex } from './metrics';
import {
  codexState,
  liveState,
  providerCatalog,
  providerCatalogIndex,
  settingsState,
} from '../test/appFixtures';

const apiKeyMocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: apiKeyMocks.invoke }));

afterEach(cleanup);
beforeEach(() => apiKeyMocks.invoke.mockResolvedValue(null));

function props() {
  const viewState: UsageViewState = structuredClone(liveState);
  const dashboardProps: DashboardProps = {
    viewState,
    settings: settingsState.settings,
    now: Date.now(),
    catalog: providerCatalogIndex,
    renamableProviderIds: settingsState.renamableProviderIds,
    onSettingsChange: vi.fn(),
    onCustomizationChange: vi.fn(),
    onReorderStart: vi.fn(),
    onReorderEnd: vi.fn(),
    onCustomize: vi.fn(),
    onOpenProviderCustomize: vi.fn(),
    onRenameProvider: vi.fn(),
    onShare: vi.fn(),
    onShareTotal: vi.fn(),
    onRefresh: vi.fn(),
    onRefreshIfDue: vi.fn(),
    onOpenProviderLink: vi.fn(),
    onContentMorph: vi.fn(),
    reducedMotion: false,
    updateStatus: null,
    installingUpdate: false,
    updateProgress: null,
    updateError: null,
    onInstallUpdate: vi.fn(),
    onOpenUpdatePage: vi.fn(),
  };
  return {
    settingsView: structuredClone(settingsState),
    dashboardProps,
    platform: 'macos' as const,
    panelHeightMode: 'automatic' as const,
    onPanelHeightModeChange: vi.fn(),
    onRequestNotifications: vi.fn(),
    onOpenNotificationSettings: vi.fn(),
    onCheckForUpdates: vi.fn(),
    checkingUpdate: false,
    onCopyLogPath: vi.fn(),
    onOpenLogFolder: vi.fn(),
    onResetAllSettings: vi.fn(),
    onResetAllCustomization: vi.fn(),
    onResetProviderCustomization: vi.fn(),
    resettingProviderId: null,
  };
}

function disabledClaude(state: SettingsViewState) {
  return {
    ...state,
    settings: {
      ...state.settings,
      providers: [
        {
          id: 'claude',
          enabled: false,
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
        ...state.settings.providers,
      ],
    },
  } satisfies SettingsViewState;
}

describe('SettingsWorkspace', () => {
  it('keeps the provider list, settings, and live preview in three columns', async () => {
    const input = props();
    render(SettingsWorkspace, input);

    const workspace = document.querySelector<HTMLElement>('[data-settings-workspace]')!;
    expect(workspace.querySelectorAll('[data-workspace-panel]')).toHaveLength(3);
    expect(screen.getByRole('progressbar', { name: 'Session used' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Refresh Codex' })).toBeInTheDocument();
    expect(
      screen.getByRole('progressbar', { name: 'Session used' }).closest('[inert]'),
    ).not.toBeInTheDocument();
    expect(
      document.querySelector('[data-provider-preview] .provider-data-view__dashboard'),
    ).not.toHaveAttribute('inert');
    expect(
      document.querySelector('[data-provider-preview] .provider-settings-button'),
    ).not.toBeInTheDocument();
    expect(
      document.querySelector('[data-provider-preview] .metric__reading button'),
    ).not.toBeInTheDocument();
    expect(input.dashboardProps.onRefresh).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole('button', { name: 'Refresh Codex' }));
    expect(input.dashboardProps.onRefresh).toHaveBeenCalledWith('codex');
  });

  it('selects a disabled provider without leaving the workspace and opens general settings', async () => {
    const input = props();
    input.settingsView = disabledClaude(input.settingsView);
    render(SettingsWorkspace, input);

    const workspace = document.querySelector<HTMLElement>('[data-settings-workspace]')!;
    await fireEvent.click(within(workspace).getByRole('button', { name: 'Claude' }));
    expect(document.querySelector('[data-settings-workspace]')).toBe(workspace);
    // The settings panel is named after the selected provider.
    expect(workspace.querySelector('[data-workspace-panel="settings"]')).toHaveAttribute(
      'aria-label',
      'Claude',
    );

    const providerNavigation = within(workspace).getByRole('navigation', { name: 'Settings' });
    await fireEvent.click(within(providerNavigation).getByRole('button', { name: /General/ }));
    expect(workspace.querySelector('[data-workspace-panel="settings"]')).toHaveAttribute(
      'aria-label',
      'App preferences',
    );
    expect(screen.getByRole('combobox', { name: 'Language' })).toBeInTheDocument();
  });

  it('checks the selected provider preview for a due background refresh', async () => {
    const input = props();
    input.settingsView = disabledClaude(input.settingsView);
    input.settingsView.settings.providers[0].enabled = true;
    input.dashboardProps.settings = input.settingsView.settings;
    render(SettingsWorkspace, input);

    await fireEvent.click(screen.getByRole('button', { name: 'Claude' }));
    expect(input.dashboardProps.onRefreshIfDue).toHaveBeenLastCalledWith('claude');
  });

  it('remounts provider credentials when switching providers', async () => {
    apiKeyMocks.invoke.mockImplementation((command: string, args?: { providerId?: string }) => {
      if (command === 'get_provider_api_key_state')
        return Promise.resolve({ providerId: args?.providerId, status: 'notSet' });
      return Promise.resolve(null);
    });
    const input = props();
    input.dashboardProps.catalog = new ProviderCatalogIndex({
      ...providerCatalog,
      apiKeyProviderIds: ['openrouter', 'codex'],
    });
    const openRouterLayout = {
      ...input.settingsView.settings.providers[0],
      id: 'openrouter',
      enabled: true,
      metrics: [],
    };
    input.settingsView.settings.providers.push(openRouterLayout);
    input.dashboardProps.settings = input.settingsView.settings;
    render(SettingsWorkspace, input);

    await fireEvent.click(screen.getByRole('button', { name: 'OpenRouter' }));
    await screen.findByRole('region', { name: 'OpenRouter API Key' });
    await fireEvent.click(screen.getByRole('button', { name: 'Add' }));
    await fireEvent.input(screen.getByLabelText('OpenRouter API key'), {
      target: { value: 'unsaved-openrouter-key' },
    });
    await fireEvent.click(screen.getByRole('button', { name: 'Codex' }));

    expect(await screen.findByRole('region', { name: 'Codex API Key' })).toBeInTheDocument();
    await waitFor(() =>
      expect(apiKeyMocks.invoke).toHaveBeenCalledWith('get_provider_api_key_state', {
        providerId: 'codex',
      }),
    );
    expect(screen.queryByDisplayValue('unsaved-openrouter-key')).not.toBeInTheDocument();
  });

  it('falls back after a selected provider is removed and updates the preview from new usage data', async () => {
    const input = props();
    input.settingsView = disabledClaude(input.settingsView);
    const { rerender } = render(SettingsWorkspace, input);
    await fireEvent.click(screen.getByRole('button', { name: 'Claude' }));
    const nextState = structuredClone(settingsState);
    await rerender({ ...input, settingsView: nextState });
    expect(screen.getByRole('button', { name: 'Codex' })).toHaveAttribute('aria-current', 'true');

    const changedView: UsageViewState = structuredClone(liveState);
    changedView.providers.codex = {
      ...codexState,
      snapshot: codexState.snapshot
        ? {
            ...codexState.snapshot,
            quotas: codexState.snapshot.quotas.map((quota, index) =>
              index === 0 ? { ...quota, usedPercent: 19 } : quota,
            ),
          }
        : null,
    };
    await rerender({
      ...input,
      settingsView: nextState,
      dashboardProps: { ...input.dashboardProps, viewState: changedView },
    });
    expect(screen.getByRole('progressbar', { name: 'Session used' })).toHaveAttribute(
      'aria-valuenow',
      '19',
    );
  });

  it('keeps navigation before the editor and preview controls for keyboard users', () => {
    const input = props();
    render(SettingsWorkspace, input);
    const workspace = document.querySelector<HTMLElement>('[data-settings-workspace]')!;
    const panels = [...workspace.querySelectorAll<HTMLElement>('[data-workspace-panel]')];
    expect(panels.map((panel) => panel.dataset.workspacePanel)).toEqual([
      'providers',
      'settings',
      'preview',
    ]);
    expect(
      panels[0].compareDocumentPosition(panels[1]) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(
      panels[1].compareDocumentPosition(panels[2]) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });
});
