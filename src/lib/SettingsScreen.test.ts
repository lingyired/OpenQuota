import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import SettingsScreen from './SettingsScreen.svelte';
import { settingsState } from '../test/appFixtures';
import type { DesktopPlatform } from './platform';
import type { SettingsViewState } from './types';

function renderSettingsScreen({
  platform,
  appMenubarForced,
  showAppMenubar,
}: {
  platform: DesktopPlatform;
  appMenubarForced: boolean;
  showAppMenubar: boolean;
}) {
  const onChange = vi.fn();
  const settingsView: SettingsViewState = {
    ...settingsState,
    appMenubarForced,
    settings: { ...settingsState.settings, showAppMenubar },
  };

  render(SettingsScreen, {
    settingsView,
    platform,
    panelHeightMode: 'automatic',
    onChange,
    onPanelHeightModeChange: vi.fn(),
    onRequestNotifications: vi.fn(),
    onOpenNotificationSettings: vi.fn(),
    updateError: null,
    checkingUpdate: false,
    onCheckForUpdates: vi.fn(),
    onCustomize: vi.fn(),
    onCopyLogPath: vi.fn(async () => {}),
    onOpenLogFolder: vi.fn(async () => {}),
    onResetAllSettings: vi.fn(),
  });

  return { onChange };
}

afterEach(cleanup);

describe('SettingsScreen app menubar setting', () => {
  it('renders on macOS and toggles showAppMenubar through the settings change path', async () => {
    const { onChange } = renderSettingsScreen({
      platform: 'macos',
      appMenubarForced: false,
      showAppMenubar: true,
    });

    const checkbox = screen.getByRole('checkbox', { name: 'Show Quota01 Menu Bar Icon' });
    expect(checkbox).toBeInTheDocument();
    expect(checkbox).toBeChecked();

    await fireEvent.click(checkbox);

    expect(onChange).toHaveBeenCalledOnce();
    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({
        showAppMenubar: false,
      }),
    );
  });

  it('renders the forced-visible explanation only when appMenubarForced is true', () => {
    renderSettingsScreen({
      platform: 'macos',
      appMenubarForced: false,
      showAppMenubar: true,
    });
    expect(
      screen.queryByText('No provider menu bar icons are visible, so Quota01 stays visible.'),
    ).not.toBeInTheDocument();
    cleanup();

    renderSettingsScreen({
      platform: 'macos',
      appMenubarForced: true,
      showAppMenubar: true,
    });
    expect(
      screen.getByText('No provider menu bar icons are visible, so Quota01 stays visible.'),
    ).toBeInTheDocument();
  });

  it('does not render the app menubar setting on Windows', () => {
    renderSettingsScreen({
      platform: 'windows',
      appMenubarForced: false,
      showAppMenubar: true,
    });

    expect(
      screen.queryByRole('checkbox', { name: 'Show Quota01 Menu Bar Icon' }),
    ).not.toBeInTheDocument();
  });
});
