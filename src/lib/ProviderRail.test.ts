import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import ProviderRail from './ProviderRail.svelte';
import type { SettingsViewState, UsageViewState } from './types';
import { claudeState, codexState, providerCatalogIndex, settingsState } from '../test/appFixtures';

afterEach(() => {
  cleanup();
  Reflect.deleteProperty(Element.prototype, 'scrollIntoView');
});

function show(selectedProviderId: string | null = null) {
  const settings: SettingsViewState['settings'] = {
    ...settingsState.settings,
    providers: [
      {
        id: 'claude',
        enabled: true,
        detected: true,
        expanded: false,
        metrics: [{ id: 'claude.session', enabled: true, section: 'alwaysVisible', pinned: true }],
      },
      settingsState.settings.providers[0],
    ],
  };
  const viewState: UsageViewState = {
    providers: { claude: claudeState, codex: codexState },
  };
  const onSelect = vi.fn();
  render(ProviderRail, {
    viewState,
    settings,
    catalog: providerCatalogIndex,
    selectedProviderId,
    onSelect,
  });
  return { onSelect };
}

describe('ProviderRail', () => {
  it('renders All and enabled providers with their pinned readings', () => {
    show();
    expect(screen.getAllByRole('tab')).toHaveLength(3);
    expect(screen.getByRole('tab', { name: 'All' })).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByRole('tab', { name: /Claude.*Session.*80%/ })).toBeInTheDocument();
    expect(
      screen.getByRole('tab', { name: /Codex.*Session.*68%.*Weekly.*41%/ }),
    ).toBeInTheDocument();
  });

  it('selects a provider on click and activates adjacent tabs with vertical arrow keys', async () => {
    const { onSelect } = show();
    const all = screen.getByRole('tab', { name: 'All' });
    const claude = screen.getByRole('tab', { name: /Claude/ });
    await fireEvent.click(claude);
    expect(onSelect).toHaveBeenLastCalledWith('claude');

    await fireEvent.keyDown(all, { key: 'ArrowDown' });
    expect(onSelect).toHaveBeenLastCalledWith('claude');
    expect(claude).toHaveFocus();
    await fireEvent.keyDown(claude, { key: 'End' });
    expect(onSelect).toHaveBeenLastCalledWith('codex');
    expect(screen.getByRole('tab', { name: /Codex/ })).toHaveFocus();
  });

  it('exposes a vertical tablist', () => {
    show();
    expect(screen.getByRole('tablist')).toHaveAttribute('aria-orientation', 'vertical');
  });

  it('scrolls the active provider square into view', async () => {
    const scrollIntoView = vi.fn();
    Object.defineProperty(Element.prototype, 'scrollIntoView', {
      configurable: true,
      value: scrollIntoView,
    });

    show('codex');

    await waitFor(() =>
      expect(scrollIntoView).toHaveBeenCalledWith({ block: 'nearest', inline: 'nearest' }),
    );
  });

  it('keeps only the selected tab tabbable', () => {
    show('codex');
    expect(screen.getByRole('tab', { name: /Codex/ })).toHaveAttribute('tabindex', '0');
    expect(screen.getByRole('tab', { name: 'All' })).toHaveAttribute('tabindex', '-1');
  });

  it('renders provider icons at the larger rail size', () => {
    show();
    expect(screen.getByRole('tab', { name: /Claude/ }).querySelector('svg')).toHaveAttribute(
      'width',
      '22',
    );
  });
});
