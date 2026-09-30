/**
 * 「最近更新成功」那一行显示什么。
 *
 * 抽成纯函数是为了能直接钉住三种状态的文案选择，而不必渲染整个 Dashboard。
 */
export interface RefreshStatus {
  /** i18n 键。 */
  key: string;
  /** i18n 参数。 */
  params?: Record<string, string | number>;
  /** 这一行是否要按「有问题」的样子呈现（失败标记）。 */
  failed: boolean;
}

/**
 * 这一行要描述的那一份数据。
 */
export interface RefreshStatusSource {
  /** 「这份数据是什么时候真的拿到的」。 */
  lastSuccessfulAt: string | null | undefined;
  /** 这份数据现在是不是处在失败状态。 */
  failed: boolean;
}

/**
 * 只是为了挑输入而用到的那几个字段，故意写成结构化类型，免得把整个 `UsageViewState`
 * 拽进来。
 */
export interface RefreshStatusViewState {
  providers: Record<
    string,
    { snapshot?: { refreshedAt?: string } | null; lastRefreshFailed?: boolean } | undefined
  >;
  lastSuccessfulRefreshAt?: string | null;
  lastRefreshFailed?: boolean;
}

/**
 * 这一行该描述谁：选中了某个 provider 就描述它自己，没选中才退回整体。
 *
 * 看 WorkBuddy 的时候不该报 Trae 的失败——那一行就在数字正上方，用户读成「我眼前这份
 * 数据新不新」。用整体结论会把别人家的故障挂到这份数据头上，反过来选中一个从没成功过
 * 的 provider 时，又会借别人的成功显示成「刚刚更新」。
 */
export function refreshStatusSource(
  viewState: RefreshStatusViewState,
  providerId: string | null | undefined,
): RefreshStatusSource {
  if (!providerId) {
    return {
      lastSuccessfulAt: viewState.lastSuccessfulRefreshAt,
      failed: viewState.lastRefreshFailed === true,
    };
  }
  const state = viewState.providers[providerId];
  return {
    lastSuccessfulAt: state?.snapshot?.refreshedAt ?? null,
    failed: state?.lastRefreshFailed === true,
  };
}

/**
 * 把「最近一次成功刷新的时刻」与「当前是否有 provider 处于失败状态」翻成一行文案。
 *
 * `lastSuccessfulAt` 为 `null` 表示从来没有成功过——这时不能显示成「刚刚更新」，
 * 否则一个完全没有数据的界面会看起来是最新的。
 */
export function refreshStatus(
  lastSuccessfulAt: string | null | undefined,
  lastRefreshFailed: boolean,
  now: number,
): RefreshStatus {
  const failed = Boolean(lastRefreshFailed);
  if (!lastSuccessfulAt) {
    return { key: 'dashboard.refreshStatus.never', failed };
  }
  const elapsedSeconds = Math.floor((now - Date.parse(lastSuccessfulAt)) / 1000);
  if (!Number.isFinite(elapsedSeconds)) {
    return { key: 'dashboard.refreshStatus.unavailable', failed };
  }
  // 时钟回拨或时间戳在未来时按「刚刚」处理，不显示负数。
  const safeSeconds = Math.max(0, elapsedSeconds);
  if (safeSeconds < 60) {
    return { key: 'dashboard.refreshStatus.momentsAgo', failed };
  }
  const minutes = Math.floor(safeSeconds / 60);
  if (minutes < 60) {
    return { key: 'dashboard.refreshStatus.minutesAgo', params: { minutes }, failed };
  }
  const hours = Math.floor(minutes / 60);
  const remainingMinutes = minutes % 60;
  return {
    key: 'dashboard.refreshStatus.hoursAgo',
    params: { hours, minutes: remainingMinutes ? ` ${remainingMinutes}m` : '' },
    failed,
  };
}
