import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import ProviderIcon from './ProviderIcon.svelte';
import UsageTrend from './UsageTrend.svelte';

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe('native visual contract', () => {
  it.each([
    'claude',
    'codex',
    'cursor',
    'antigravity',
    'copilot',
    'devin',
    'grok',
    'opencode',
    'openrouter',
    'zai',
    'workbuddy',
    'deepseek',
    'trae-cn',
  ])('packages the exact %s provider icon', (providerId) => {
    const { container } = render(ProviderIcon, { providerId });
    const icon = container.querySelector('.provider-icon');
    expect(icon).not.toBeNull();
    const path = icon?.querySelector('path')?.getAttribute('d');
    expect(icon?.getAttribute('src')?.length ?? path?.length ?? 0).toBeGreaterThan(10);
  });

  it.each([
    ['antigravity', 'antigravity'],
    ['claude', 'claude'],
    ['codex', 'codex'],
    ['copilot', 'copilot'],
    ['devin', 'devin'],
    ['kimi', 'kimi'],
    ['minimax', 'minimax'],
    ['openrouter', 'openrouter'],
    ['workbuddy', 'codebuddy'],
  ])('uses the full-color Lobe asset for %s', (providerId, slug) => {
    const { container } = render(ProviderIcon, { providerId });
    const icon = container.querySelector<HTMLImageElement>('img.provider-icon');
    expect(icon).not.toBeNull();
    expect(icon).toHaveAttribute('data-icon', slug);
    expect(icon?.getAttribute('src')).toMatch(/-color\.svg|^data:image\/svg\+xml/);
  });

  it('keeps theme-aware mono fallbacks where Lobe has no color variant', () => {
    const cursor = render(ProviderIcon, { providerId: 'cursor' });
    expect(cursor.container.querySelector('path')).toHaveAttribute('fill', 'currentColor');
    cleanup();
    const grok = render(ProviderIcon, { providerId: 'grok' });
    expect(grok.container.querySelector('path')).toHaveAttribute('fill', 'currentColor');
    cleanup();
    const opencode = render(ProviderIcon, { providerId: 'opencode' });
    expect(opencode.container.querySelector('path')).toHaveAttribute('fill', 'currentColor');
    cleanup();
    const zai = render(ProviderIcon, { providerId: 'zai' });
    expect(zai.container.querySelector('path')).toHaveAttribute('fill', 'currentColor');
    cleanup();
    const deepseek = render(ProviderIcon, { providerId: 'deepseek' });
    expect(deepseek.container.querySelector('path')).toHaveAttribute('fill', 'currentColor');
    cleanup();
    const trae = render(ProviderIcon, { providerId: 'trae-cn' });
    expect(trae.container.querySelector('path')).toHaveAttribute('fill', 'currentColor');
  });

  it('reuses the Claude mark for Claude account cards', () => {
    const claude = render(ProviderIcon, { providerId: 'claude' });
    const account = render(ProviderIcon, { providerId: 'claude@1234abcd' });

    expect(account.container.innerHTML).toBe(claude.container.innerHTML);
  });

  it('uses the shared hover dwell and grace timing for Usage Trend details', async () => {
    vi.useFakeTimers();
    const today = new Date();
    const date = `${today.getFullYear()}-${String(today.getMonth() + 1).padStart(2, '0')}-${String(today.getDate()).padStart(2, '0')}`;
    render(UsageTrend, {
      daily: [{ date, tokens: 42_000, estimatedCostUsd: 0.21, estimateComplete: true }],
      sourceNote: 'From your Codex logs (estimated)',
    });
    const chart = screen.getByRole('group', { name: 'Usage trend chart details' });

    await fireEvent.mouseEnter(chart);
    await vi.advanceTimersByTimeAsync(399);
    expect(screen.queryByText('peak 42K tokens')).not.toBeInTheDocument();
    await vi.advanceTimersByTimeAsync(1);
    expect(screen.getByText('peak 42K tokens')).toBeInTheDocument();
    expect(screen.getByText('From your Codex logs (estimated)')).toBeInTheDocument();

    await fireEvent.mouseLeave(chart);
    await vi.advanceTimersByTimeAsync(179);
    expect(screen.getByText('peak 42K tokens')).toBeInTheDocument();
    await vi.advanceTimersByTimeAsync(1);
    expect(screen.queryByText('peak 42K tokens')).not.toBeInTheDocument();
  });

  it('reveals an exact day value when a detail bar is hovered', async () => {
    vi.useFakeTimers();
    const today = new Date();
    const date = `${today.getFullYear()}-${String(today.getMonth() + 1).padStart(2, '0')}-${String(today.getDate()).padStart(2, '0')}`;
    const { container } = render(UsageTrend, {
      daily: [{ date, tokens: 42_000, estimatedCostUsd: 0.21, estimateComplete: true }],
      sourceNote: 'From your Codex logs (estimated)',
    });
    await fireEvent.mouseEnter(screen.getByRole('group', { name: 'Usage trend chart details' }));
    await vi.advanceTimersByTimeAsync(400);
    const bars = container.querySelectorAll<HTMLElement>('.trend-detail__bars i');
    await fireEvent.mouseEnter(bars[bars.length - 1]);
    expect(screen.getByText(/· 42K tokens$/)).toBeInTheDocument();
    expect(container.querySelectorAll('.trend-detail__bars i.muted')).toHaveLength(30);
  });
});
