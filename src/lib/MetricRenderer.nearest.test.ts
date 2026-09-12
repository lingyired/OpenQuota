import { cleanup, render, screen } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import MetricRenderer from './MetricRenderer.svelte';
import { ProviderCatalogIndex } from './metrics';
import { settingsState } from '../test/appFixtures';
import type { ProviderCatalog, ProviderSnapshot } from './types';

afterEach(cleanup);

const catalog: ProviderCatalog = {
  providers: [
    {
      id: 'workbuddy',
      displayName: 'Workbuddy CN',
      shortName: 'WB',
      fallbackEnabled: false,
      localUsageSourceNote: null,
      links: [],
      metrics: [
        {
          id: 'workbuddy.nearestExpiring',
          label: '近期到期的积分包',
          source: { kind: 'nearestCreditPackage', sourceId: 'nearestExpiring' },
          pinnable: true,
          defaultEnabled: true,
          defaultSection: 'alwaysVisible',
          defaultPinned: true,
          tray: { shortLabel: '近', suffix: null },
        },
      ],
    },
  ],
};

const snapshot: ProviderSnapshot = {
  providerId: 'workbuddy',
  plan: null,
  quotas: [],
  creditPackages: [
    {
      code: 'nearest',
      name: '每月登录赠送 · 通用',
      total: 500,
      remaining: 447.78,
      used: 52.22,
      expiresAt: '2026-09-30T15:59:59Z',
    },
  ],
  valueMetrics: [],
  statusMetrics: [],
  notices: [],
  usage: {
    today: null,
    yesterday: null,
    last30Days: null,
    daily: [],
    unknownModels: [],
  },
  warnings: [],
  refreshedAt: '2026-09-12T15:00:00Z',
};

describe('MetricRenderer nearest credit package', () => {
  it('renders the nearest package with a progress meter and expiry', () => {
    render(MetricRenderer, {
      layout: {
        id: 'workbuddy.nearestExpiring',
        enabled: true,
        section: 'alwaysVisible',
        pinned: true,
      },
      snapshot,
      settings: settingsState.settings,
      now: new Date('2026-09-12T15:00:00Z').getTime(),
      catalog: new ProviderCatalogIndex(catalog),
      onSettingsChange: vi.fn(),
      expanded: true,
    });

    expect(screen.getByRole('heading', { name: '近期到期的积分包' })).toBeInTheDocument();
    expect(screen.getByText('447.78 / 500')).toBeInTheDocument();
    expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '89.556');
    expect(screen.getByText(/到期 /)).toBeInTheDocument();
  });
});
