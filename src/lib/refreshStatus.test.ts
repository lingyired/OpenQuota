import { describe, expect, it } from 'vitest';

import { en } from './i18n/messages/en';
import { refreshStatus, refreshStatusSource } from './refreshStatus';

const NOW = Date.parse('2026-09-28T22:00:00Z');

function at(offsetSeconds: number): string {
  return new Date(NOW - offsetSeconds * 1000).toISOString();
}

function providers(
  entries: Record<
    string,
    { refreshedAt?: string; lastRefreshFailed?: boolean; hasSnapshot?: boolean }
  >,
) {
  return Object.fromEntries(
    Object.entries(entries).map(([id, entry]) => [
      id,
      {
        snapshot: entry.hasSnapshot === false ? null : { refreshedAt: entry.refreshedAt },
        lastRefreshFailed: entry.lastRefreshFailed,
      },
    ]),
  );
}

describe('refreshStatus', () => {
  it('reports moments ago for a refresh inside the last minute', () => {
    expect(refreshStatus(at(5), false, NOW)).toEqual({
      key: 'dashboard.refreshStatus.momentsAgo',
      failed: false,
    });
  });

  it('reports minutes for a refresh inside the last hour', () => {
    expect(refreshStatus(at(3 * 60), false, NOW)).toEqual({
      key: 'dashboard.refreshStatus.minutesAgo',
      params: { minutes: 3 },
      failed: false,
    });
  });

  it('reports hours and the leftover minutes beyond an hour', () => {
    expect(refreshStatus(at(2 * 3600 + 5 * 60), false, NOW)).toEqual({
      key: 'dashboard.refreshStatus.hoursAgo',
      params: { hours: 2, minutes: ' 5m' },
      failed: false,
    });
  });

  it('omits the leftover minutes when the hour is exact', () => {
    expect(refreshStatus(at(2 * 3600), false, NOW)).toEqual({
      key: 'dashboard.refreshStatus.hoursAgo',
      params: { hours: 2, minutes: '' },
      failed: false,
    });
  });

  // 从未成功过时绝不能显示成「刚刚更新」：那会让一个空界面看起来是最新的。
  it('never claims a fresh update when nothing has ever succeeded', () => {
    expect(refreshStatus(null, true, NOW)).toEqual({
      key: 'dashboard.refreshStatus.never',
      failed: true,
    });
    expect(refreshStatus(undefined, false, NOW).key).toBe('dashboard.refreshStatus.never');
  });

  it('carries the failure flag through every state', () => {
    expect(refreshStatus(at(5), true, NOW).failed).toBe(true);
    expect(refreshStatus(at(3 * 60), true, NOW).failed).toBe(true);
    expect(refreshStatus(at(2 * 3600), true, NOW).failed).toBe(true);
  });

  it('falls back when the timestamp cannot be parsed', () => {
    expect(refreshStatus('not-a-date', false, NOW).key).toBe('dashboard.refreshStatus.unavailable');
  });

  // 时钟回拨时时间戳会落在未来，此时不能算出负数。
  it('treats a future timestamp as just now instead of a negative age', () => {
    expect(refreshStatus(at(-30), false, NOW).key).toBe('dashboard.refreshStatus.momentsAgo');
  });
});

describe('refreshStatusSource', () => {
  it('describes the whole panel when no provider is selected', () => {
    const state = {
      providers: providers({ trae: { refreshedAt: at(5), lastRefreshFailed: true } }),
      lastSuccessfulRefreshAt: at(30),
      lastRefreshFailed: true,
    };
    expect(refreshStatusSource(state, null)).toEqual({
      lastSuccessfulAt: at(30),
      failed: true,
    });
    expect(refreshStatusSource(state, undefined).lastSuccessfulAt).toBe(at(30));
  });

  // 这是用户报的那个场景：看 WorkBuddy（好的）时，不能因为 Trae 挂了就报失败。
  it('ignores another providers failure when one is selected', () => {
    const state = {
      providers: providers({
        workbuddy: { refreshedAt: at(5), lastRefreshFailed: false },
        trae: { refreshedAt: at(600), lastRefreshFailed: true },
      }),
      lastSuccessfulRefreshAt: at(5),
      lastRefreshFailed: true,
    };
    expect(refreshStatusSource(state, 'workbuddy')).toEqual({
      lastSuccessfulAt: at(5),
      failed: false,
    });
  });

  // 反面：选中那个坏的时候要如实报出来。
  it('reports the selected providers own failure', () => {
    const state = {
      providers: providers({
        workbuddy: { refreshedAt: at(5), lastRefreshFailed: false },
        trae: { refreshedAt: at(600), lastRefreshFailed: true },
      }),
      lastSuccessfulRefreshAt: at(5),
      lastRefreshFailed: false,
    };
    expect(refreshStatusSource(state, 'trae')).toEqual({
      lastSuccessfulAt: at(600),
      failed: true,
    });
  });

  // 选中的 provider 从没成功过时，绝不能借别人的成功显示成「刚刚更新」。
  it('never borrows another providers timestamp for a provider without data', () => {
    const state = {
      providers: providers({
        workbuddy: { refreshedAt: at(5), lastRefreshFailed: false },
        trae: { hasSnapshot: false, lastRefreshFailed: true },
      }),
      lastSuccessfulRefreshAt: at(5),
      lastRefreshFailed: false,
    };
    const source = refreshStatusSource(state, 'trae');
    expect(source.lastSuccessfulAt).toBeNull();
    expect(source.failed).toBe(true);
    expect(refreshStatus(source.lastSuccessfulAt, source.failed, NOW).key).toBe(
      'dashboard.refreshStatus.never',
    );
  });

  it('treats a provider with no state yet as never updated and not failed', () => {
    const state = { providers: {}, lastSuccessfulRefreshAt: at(5), lastRefreshFailed: false };
    expect(refreshStatusSource(state, 'missing')).toEqual({
      lastSuccessfulAt: null,
      failed: false,
    });
  });
});

describe('refreshStatus translations', () => {
  // 这些键由 refreshStatus() 动态返回，静态检查看不见它们。少一个键时界面会退化成
  // 显示原始键名（例如「dashboard.refreshStatus.never」），而 svelte-check 不会报错。
  it('has a translation for every key the formatter can return', () => {
    const messages = en.dashboard.refreshStatus as Record<string, string>;
    const produced = new Set([
      refreshStatus(null, false, NOW).key,
      refreshStatus(at(5), false, NOW).key,
      refreshStatus(at(3 * 60), false, NOW).key,
      refreshStatus(at(2 * 3600), false, NOW).key,
      refreshStatus('not-a-date', false, NOW).key,
    ]);
    expect(produced.size).toBe(5);

    for (const key of produced) {
      const short = key.replace('dashboard.refreshStatus.', '');
      expect(messages[short], `${key} is missing from the en dictionary`).toBeTruthy();
    }
  });

  // 模板里直接引用的键（不在 refreshStatus() 的返回值里）。
  it('has the keys the popup row renders directly', () => {
    const messages = en.dashboard.refreshStatus as Record<string, string>;
    for (const key of ['failed', 'refresh', 'refreshing', 'refreshAll']) {
      expect(messages[key], `dashboard.refreshStatus.${key} is missing`).toBeTruthy();
    }
  });
});
