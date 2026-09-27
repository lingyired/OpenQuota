import { cleanup, render, screen } from '@testing-library/svelte';
import { afterEach, describe, expect, it } from 'vitest';
import SettingsScreen from './SettingsScreen.svelte';
import { settingsState } from '../test/appFixtures';

afterEach(cleanup);

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
