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
  proxyUrl = 'http://127.0.0.1:8888',
}: {
  platform: DesktopPlatform;
  appMenubarForced: boolean;
  showAppMenubar: boolean;
  proxyUrl?: string | null;
}) {
  const onChange = vi.fn();
  const settingsView: SettingsViewState = {
    ...settingsState,
    appMenubarForced,
    settings: { ...settingsState.settings, showAppMenubar, proxyUrl },
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

  return { onChange, settingsView };
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

describe('SettingsScreen proxy URL setting', () => {
  it('shows the saved URL and commits a trimmed URL on blur', async () => {
    const { onChange } = renderSettingsScreen({
      platform: 'linux',
      appMenubarForced: false,
      showAppMenubar: true,
      proxyUrl: 'http://127.0.0.1:8888',
    });

    const input = screen.getByRole('textbox', { name: 'Proxy URL' });
    expect(input).toHaveValue('http://127.0.0.1:8888');

    await fireEvent.input(input, { target: { value: ' http://127.0.0.1:8080 ' } });
    await fireEvent.blur(input);

    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({ proxyUrl: 'http://127.0.0.1:8080' }),
    );
  });

  it('commits the trimmed URL on Enter', async () => {
    const { onChange } = renderSettingsScreen({
      platform: 'linux',
      appMenubarForced: false,
      showAppMenubar: true,
      proxyUrl: null,
    });
    const input = screen.getByRole('textbox', { name: 'Proxy URL' });

    await fireEvent.input(input, { target: { value: ' http://proxy.example:9000 ' } });
    await fireEvent.keyDown(input, { key: 'Enter' });

    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({ proxyUrl: 'http://proxy.example:9000' }),
    );
  });

  it('keeps an invalid edit draft while the parent retains its last saved URL', async () => {
    const { onChange, settingsView } = renderSettingsScreen({
      platform: 'linux',
      appMenubarForced: false,
      showAppMenubar: true,
      proxyUrl: 'http://127.0.0.1:8888',
    });
    const input = screen.getByRole('textbox', { name: 'Proxy URL' });

    await fireEvent.input(input, { target: { value: 'not a valid URL' } });
    await fireEvent.blur(input);

    expect(onChange).toHaveBeenCalledWith(expect.objectContaining({ proxyUrl: 'not a valid URL' }));
    expect(input).toHaveValue('not a valid URL');
    expect(settingsView.settings.proxyUrl).toBe('http://127.0.0.1:8888');
  });
});
