import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { addMessages, locale } from 'svelte-i18n';
import SettingsScreen from './SettingsScreen.svelte';
import { settingsState } from '../test/appFixtures';
import type { DesktopPlatform } from './platform';
import type { SettingsViewState } from './types';
import { t } from './i18n';

afterEach(cleanup);
beforeEach(() => locale.set('en'));

describe('SettingsScreen', () => {
  it('does not offer a Quota01 app-level menu bar icon setting', () => {
    render(SettingsScreen, {
      settingsView: settingsState,
      platform: 'macos',
      panelHeightMode: 'automatic',
      onChange: () => {},
      onPanelHeightModeChange: () => {},
      onRequestNotifications: () => {},
      onOpenNotificationSettings: () => {},
      updateError: null,
      checkingUpdate: false,
      onCheckForUpdates: () => {},
      onCopyLogPath: async () => {},
      onOpenLogFolder: async () => {},
      onResetAllSettings: () => {},
      onResetAllCustomization: () => {},
    });

    expect(screen.queryByText(/Quota01 Menu Bar Icon/)).not.toBeInTheDocument();
  });
});

function renderSettingsScreen({
  platform,
  proxyUrl = 'http://127.0.0.1:8888',
}: {
  platform: DesktopPlatform;
  proxyUrl?: string | null;
}) {
  const onChange = vi.fn();
  const settingsView: SettingsViewState = {
    ...settingsState,
    settings: { ...settingsState.settings, proxyUrl },
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
    onCopyLogPath: vi.fn(async () => {}),
    onOpenLogFolder: vi.fn(async () => {}),
    onResetAllSettings: vi.fn(),
    onResetAllCustomization: vi.fn(),
  });

  return { onChange, settingsView };
}

describe('SettingsScreen proxy URL setting', () => {
  it('shows the saved URL and commits a trimmed URL on blur', async () => {
    const { onChange } = renderSettingsScreen({
      platform: 'linux',
      proxyUrl: 'http://127.0.0.1:8888',
    });

    const input = screen.getByRole('textbox', { name: t('settings.proxyUrl') });
    expect(input).toHaveValue('http://127.0.0.1:8888');

    await fireEvent.input(input, { target: { value: ' http://127.0.0.1:8080 ' } });
    await fireEvent.blur(input);

    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({ proxyUrl: 'http://127.0.0.1:8080' }),
    );
  });

  it('commits the trimmed URL on Enter', async () => {
    const { onChange } = renderSettingsScreen({ platform: 'linux', proxyUrl: null });
    const input = screen.getByRole('textbox', { name: t('settings.proxyUrl') });

    await fireEvent.input(input, { target: { value: ' http://proxy.example:9000 ' } });
    await fireEvent.keyDown(input, { key: 'Enter' });

    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({ proxyUrl: 'http://proxy.example:9000' }),
    );
  });

  it('keeps an invalid edit draft while the parent retains its saved URL', async () => {
    const { onChange, settingsView } = renderSettingsScreen({
      platform: 'linux',
      proxyUrl: 'http://127.0.0.1:8888',
    });
    const input = screen.getByRole('textbox', { name: t('settings.proxyUrl') });

    await fireEvent.input(input, { target: { value: 'not a valid URL' } });
    await fireEvent.blur(input);

    expect(onChange).toHaveBeenCalledWith(expect.objectContaining({ proxyUrl: 'not a valid URL' }));
    expect(input).toHaveValue('not a valid URL');
    expect(settingsView.settings.proxyUrl).toBe('http://127.0.0.1:8888');
  });

  it('uses the active locale for the proxy URL label and helper text', async () => {
    addMessages('task-proxy-settings', {
      settings: {
        proxyUrl: 'Localized proxy URL',
        proxyUrlHelp: 'Providers use this localized shared URL when proxying.',
      },
    });
    await locale.set('task-proxy-settings');
    renderSettingsScreen({ platform: 'linux', proxyUrl: null });

    expect(screen.getByRole('textbox', { name: 'Localized proxy URL' })).toBeInTheDocument();
    expect(
      screen.getByText('Providers use this localized shared URL when proxying.'),
    ).toBeInTheDocument();
  });
});
