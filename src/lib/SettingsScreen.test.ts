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
  settings: overrides,
}: {
  platform: DesktopPlatform;
  proxyUrl?: string | null;
  settings?: Partial<SettingsViewState['settings']>;
}) {
  const onChange = vi.fn();
  const settingsView: SettingsViewState = {
    ...settingsState,
    settings: { ...settingsState.settings, proxyUrl, ...overrides },
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

const hiddenTaskbandStyleLabels = [
  'settings.labelSpacing',
  'settings.leftEdgeMargin',
  'settings.rightEdgeMargin',
] as const;

describe('SettingsScreen taskbar surface (Windows: toggle + position only)', () => {
  it('keeps the enable toggle and the position selector on Windows', () => {
    renderSettingsScreen({ platform: 'windows' });

    expect(
      screen.getByRole('checkbox', { name: t('settings.showMonitorsOnTaskbar') }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole('combobox', { name: t('settings.defaultPosition') }),
    ).toBeInTheDocument();
  });

  it('hides label spacing and both edge margins on Windows', () => {
    renderSettingsScreen({ platform: 'windows' });

    for (const key of hiddenTaskbandStyleLabels) {
      expect(screen.queryByText(t(key))).not.toBeInTheDocument();
      expect(screen.queryByRole('spinbutton', { name: t(key) })).not.toBeInTheDocument();
    }
  });

  it('toggling the Windows switch persists only the enable flag and keeps hidden values', async () => {
    const { onChange } = renderSettingsScreen({
      platform: 'windows',
      settings: {
        taskband: {
          enabled: true,
          defaultSide: 'left',
          margin: 17,
          edgeMarginLeft: 23,
          edgeMarginRight: 29,
        },
      },
    });

    await fireEvent.click(
      screen.getByRole('checkbox', { name: t('settings.showMonitorsOnTaskbar') }),
    );

    const next = onChange.mock.calls.at(-1)?.[0];
    // The toggle must flip `enabled` while every hidden-but-persisted style value
    // survives the round trip untouched.
    expect(next.taskband).toEqual({
      enabled: false,
      defaultSide: 'left',
      margin: 17,
      edgeMarginLeft: 23,
      edgeMarginRight: 29,
    });
  });
});

describe('SettingsScreen menu bar surface (macOS: enable toggle only)', () => {
  it('renders exactly one control: the enable toggle', () => {
    renderSettingsScreen({ platform: 'macos' });

    const section = screen
      .getByText(t('settings.showMonitorsOnTaskbar'))
      .closest('.settings-section');
    expect(section).not.toBeNull();

    const controls = section!.querySelectorAll('input, select, button, [role="combobox"]');
    expect(controls).toHaveLength(1);
    expect(controls[0]).toHaveAttribute('type', 'checkbox');
  });

  it('hides the position selector on macOS', () => {
    renderSettingsScreen({ platform: 'macos' });

    expect(
      screen.queryByRole('combobox', { name: t('settings.defaultPosition') }),
    ).not.toBeInTheDocument();
  });

  it('hides label spacing and both edge margins on macOS', () => {
    renderSettingsScreen({ platform: 'macos' });

    for (const key of hiddenTaskbandStyleLabels) {
      expect(screen.queryByText(t(key))).not.toBeInTheDocument();
      expect(screen.queryByRole('spinbutton', { name: t(key) })).not.toBeInTheDocument();
    }
  });

  it('drives the persisted taskband.enabled master switch that gates macOS menu-bar instances', async () => {
    const { onChange } = renderSettingsScreen({
      platform: 'macos',
      settings: {
        taskband: {
          enabled: true,
          defaultSide: 'right',
          margin: 11,
          edgeMarginLeft: 13,
          edgeMarginRight: 15,
        },
      },
    });

    await fireEvent.click(
      screen.getByRole('checkbox', { name: t('settings.showMonitorsOnTaskbar') }),
    );

    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({
        taskband: {
          enabled: false,
          defaultSide: 'right',
          margin: 11,
          edgeMarginLeft: 13,
          edgeMarginRight: 15,
        },
      }),
    );
  });

  it('reflects the persisted value so an off switch renders as unchecked', () => {
    renderSettingsScreen({
      platform: 'macos',
      settings: {
        taskband: {
          enabled: false,
          defaultSide: 'right',
          margin: 4,
          edgeMarginLeft: 0,
          edgeMarginRight: 0,
        },
      },
    });

    expect(
      screen.getByRole('checkbox', { name: t('settings.showMonitorsOnTaskbar') }),
    ).not.toBeChecked();
  });

  it('offers no menu bar surface on other platforms', () => {
    renderSettingsScreen({ platform: 'linux' });

    expect(
      screen.queryByRole('checkbox', { name: t('settings.showMonitorsOnTaskbar') }),
    ).not.toBeInTheDocument();
  });
});
