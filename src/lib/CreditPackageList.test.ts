import { cleanup, render, screen } from '@testing-library/svelte';
import { afterEach, describe, expect, it } from 'vitest';
import CreditPackageList from './CreditPackageList.svelte';

afterEach(cleanup);

describe('CreditPackageList', () => {
  it('shows only the three nearest positive-balance packages while collapsed', () => {
    render(CreditPackageList, {
      packages: [
        {
          code: 'first',
          name: '第一个积分包',
          total: 100,
          remaining: 71.38,
          used: 28.62,
          expiresAt: '2026-10-08T15:59:59Z',
        },
        {
          code: 'second',
          name: '第二个积分包',
          total: 100,
          remaining: 60,
          used: 40,
          expiresAt: '2026-10-15T15:59:59Z',
        },
        {
          code: 'third',
          name: '第三个积分包',
          total: 100,
          remaining: 50,
          used: 50,
          expiresAt: '2026-10-22T15:59:59Z',
        },
        {
          code: 'fourth',
          name: '第四个积分包',
          total: 100,
          remaining: 40,
          used: 60,
          expiresAt: '2026-10-29T15:59:59Z',
        },
      ],
    });

    expect(screen.getByText('第一个积分包')).toBeInTheDocument();
    expect(screen.getByText('71.38 / 100')).toBeInTheDocument();
    expect(screen.getByText('到期 2026/10/08')).toBeInTheDocument();
    expect(screen.getByText('第二个积分包')).toBeInTheDocument();
    expect(screen.getByText('第三个积分包')).toBeInTheDocument();
    expect(screen.queryByText('第四个积分包')).not.toBeInTheDocument();
    expect(screen.getAllByRole('progressbar')).toHaveLength(3);
  });

  it('shows unlimited credit packages without a finite progress meter', () => {
    const { container } = render(CreditPackageList, {
      packages: [
        {
          code: 'unlimited',
          name: '免费 · 通用',
          total: 0,
          remaining: 0,
          used: 0,
          expiresAt: '2026-10-08T15:59:59Z',
          unlimited: true,
        },
      ],
    });

    expect(screen.getByText('免费 · 通用')).toBeInTheDocument();
    expect(screen.getByText('无限')).toBeInTheDocument();
    expect(screen.queryByRole('progressbar')).not.toBeInTheDocument();
    expect(container.querySelector('.credit-package')).not.toBeNull();
  });

  it('shows every positive-balance package when the provider is expanded', () => {
    const { rerender } = render(CreditPackageList, {
      expanded: false,
      packages: [
        {
          code: 'first',
          name: '第一个积分包',
          total: 100,
          remaining: 71.38,
          used: 28.62,
          expiresAt: '2026-10-08T15:59:59Z',
        },
        {
          code: 'second',
          name: '第二个积分包',
          total: 100,
          remaining: 60,
          used: 40,
          expiresAt: '2026-10-15T15:59:59Z',
        },
        {
          code: 'third',
          name: '第三个积分包',
          total: 100,
          remaining: 50,
          used: 50,
          expiresAt: '2026-10-22T15:59:59Z',
        },
        {
          code: 'fourth',
          name: '第四个积分包',
          total: 100,
          remaining: 40,
          used: 60,
          expiresAt: '2026-10-29T15:59:59Z',
        },
      ],
    });

    expect(screen.queryByText('第四个积分包')).not.toBeInTheDocument();
    rerender({ expanded: true });
    expect(screen.getByText('第四个积分包')).toBeInTheDocument();
    expect(screen.getAllByRole('progressbar')).toHaveLength(4);
  });
});
