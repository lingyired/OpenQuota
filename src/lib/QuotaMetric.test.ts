import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import QuotaMetric from './QuotaMetric.svelte';
import type { QuotaWindow } from './types';

const now = Date.parse('2026-07-10T12:00:00Z');
const periodSeconds = 10_000;

afterEach(cleanup);

function quota(usedPercent: number, elapsedFraction = 0.5): QuotaWindow {
  return {
    id: 'weekly',
    label: 'Weekly',
    usedPercent,
    format: 'percent',
    usedValue: null,
    limitValue: null,
    estimated: false,
    periodSeconds,
    resetsAt: new Date(now + (1 - elapsedFraction) * periodSeconds * 1000).toISOString(),
  };
}

function show(value: QuotaWindow, onToggleReset = vi.fn(), isSessionWindow = false) {
  return {
    onToggleReset,
    ...render(QuotaMetric, {
      quota: value,
      now,
      usageDisplay: 'left',
      resetDisplay: 'countdown',
      timeFormat: 'system',
      alwaysShowPacing: false,
      isSessionWindow,
      onToggleUsage: vi.fn(),
      onToggleReset,
    }),
  };
}

function showFloored(value: QuotaWindow, usageDisplay: 'used' | 'left', isSessionWindow: boolean) {
  return render(QuotaMetric, {
    quota: value,
    now,
    usageDisplay,
    resetDisplay: 'countdown',
    timeFormat: 'system',
    alwaysShowPacing: false,
    isSessionWindow,
    percentFloored: true,
    onToggleUsage: vi.fn(),
    onToggleReset: vi.fn(),
  });
}

function showAlways(value: QuotaWindow) {
  return render(QuotaMetric, {
    quota: value,
    now,
    usageDisplay: 'left',
    resetDisplay: 'countdown',
    timeFormat: 'system',
    alwaysShowPacing: true,
    isSessionWindow: false,
    onToggleUsage: vi.fn(),
    onToggleReset: vi.fn(),
  });
}

describe('quota pacing presentation', () => {
  it('shows the flame, run-out time, and projection tooltip', async () => {
    const onToggleReset = vi.fn();
    const { container } = show(quota(60), onToggleReset);
    const warning = screen.getByRole('button', { name: 'Limit in 56m' });
    expect(container.querySelector('.pace-warning__icon')).toBeInTheDocument();
    expect(warning).toHaveAttribute('data-tooltip', '~20% over limit at reset');
    expect(container.querySelector('.meter-shell')).toHaveAttribute(
      'data-tooltip',
      '~20% over limit at reset',
    );
    expect(container.querySelector('.meter__fill')).toHaveStyle('--fill-percent: 40%');
    expect(screen.getByRole('button', { name: '40% left' })).toHaveAttribute(
      'data-tooltip',
      '60% used',
    );
    expect(screen.getByRole('button', { name: /Resets in/ })).toHaveAttribute(
      'data-tooltip',
      expect.stringContaining('Resets today at'),
    );
    await fireEvent.click(warning);
    expect(onToggleReset).toHaveBeenCalledOnce();
  });

  it('shows a flame without a misleading time at the exact-limit edge', () => {
    const { container } = show(quota(50));
    expect(screen.getByLabelText('Will reach limit')).toBeInTheDocument();
    expect(screen.queryByText(/Limit in/)).not.toBeInTheDocument();
    expect(container.querySelector('.pace-warning')).toHaveAttribute(
      'data-tooltip',
      '~100% used at reset',
    );
  });

  it('shows the rounded spare copy and projected-use tooltip when close', () => {
    show(quota(46));
    expect(screen.getByText('~8% spare')).toHaveAttribute('data-tooltip', '~92% used at reset');
  });

  it('frames the even-pace tick in the selected remaining mode', () => {
    const { container } = show(quota(30, 0.25));
    expect(container.querySelector('.meter__pace')).toHaveStyle('--pace-percent: 75%');
  });

  it('shows an unused rolling session as not started without pacing decoration', () => {
    const { container } = show(quota(0), vi.fn(), true);
    expect(screen.getByText('Not started')).toHaveAttribute(
      'data-tooltip',
      'Sessions start after you send your first message.',
    );
    expect(container.querySelector('.pace-warning')).not.toBeInTheDocument();
    expect(container.querySelector('.meter-shell')).not.toHaveAttribute('data-tooltip');
  });

  it('treats a floored zero as sub-one-percent usage instead of a fresh session', () => {
    const { container } = showFloored(quota(0), 'left', true);
    expect(screen.queryByText('Not started')).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: '>99% left' })).toHaveAttribute(
      'data-tooltip',
      '<1% used',
    );
    expect(container.querySelector('.pace-warning')).not.toBeInTheDocument();
  });

  it('shows sub-one-percent usage when the display mode is used', () => {
    showFloored(quota(0), 'used', false);
    expect(screen.getByRole('button', { name: '<1% used' })).toHaveAttribute(
      'data-tooltip',
      '>99% left',
    );
  });

  it('keeps whole-percent readings once a floored value reaches one percent', () => {
    showFloored(quota(1), 'used', true);
    expect(screen.getByRole('button', { name: '1% used' })).toBeInTheDocument();
  });

  it('does not decorate unused non-session quotas as healthy pacing', () => {
    const { container } = showAlways(quota(0));
    expect(screen.queryByText(/left at reset/)).not.toBeInTheDocument();
    expect(container.querySelector('.meter__pace')).not.toBeInTheDocument();
    expect(container.querySelector('.meter-shell')).not.toHaveAttribute('data-tooltip');
  });

  it('renders provider-supplied count units instead of a hardcoded request label', () => {
    show({
      ...quota(24),
      id: 'requests',
      label: 'Requests',
      format: 'count',
      usedValue: 120,
      limitValue: 500,
      unit: 'searches',
    });
    expect(screen.getByRole('button', { name: '380 searches left' })).toHaveAttribute(
      'data-tooltip',
      '120 searches used',
    );
  });

  it('marks inferred quotas with their source note', () => {
    show({
      ...quota(24),
      estimated: true,
      sourceNote: 'Estimated from local records.',
    });

    expect(screen.getByLabelText('Estimated quota')).toHaveAttribute(
      'data-tooltip',
      'Estimated from local records.',
    );
  });
});
