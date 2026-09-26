import { cleanup, fireEvent, render, screen, within } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import SettingsWorkspace from './SettingsWorkspace.svelte';
import type { DashboardProps } from './Dashboard.svelte';
import type { SettingsViewState, UsageViewState } from './types';
import { codexState, liveState, providerCatalogIndex, settingsState } from '../test/appFixtures';

afterEach(cleanup);

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
          keychainAccessGranted: false,
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
    expect(workspace.querySelectorAll('[data-workspace-column]')).toHaveLength(3);
    expect(screen.getByRole('progressbar', { name: 'Session used' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Refresh Codex' })).toBeInTheDocument();
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
    expect(workspace.querySelector('[data-workspace-column="settings"]')).toHaveAttribute(
      'aria-label',
      'Customize',
    );

    await fireEvent.click(within(workspace).getByRole('button', { name: 'Settings' }));
    expect(workspace.querySelector('[data-workspace-column="settings"]')).toHaveAttribute(
      'aria-label',
      'Settings',
    );
    expect(screen.getByRole('combobox', { name: 'Language' })).toBeInTheDocument();
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

  it('places the left navigation before middle and preview controls for keyboard users', () => {
    const input = props();
    render(SettingsWorkspace, input);
    const workspace = document.querySelector<HTMLElement>('[data-settings-workspace]')!;
    const columns = [...workspace.querySelectorAll<HTMLElement>('[data-workspace-column]')];
    expect(columns.map((column) => column.dataset.workspaceColumn)).toEqual([
      'providers',
      'settings',
      'preview',
    ]);
    expect(
      columns[0].compareDocumentPosition(columns[1]) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(
      columns[1].compareDocumentPosition(columns[2]) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });
});
