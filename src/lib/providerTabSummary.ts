import { formatMetricRailValue, formatMetricValue } from './metricFormat';
import type { ProviderCatalogIndex } from './metrics';
import { t } from './i18n';
import type {
  AppSettings,
  MetricDefinition,
  MetricLayout,
  ProviderLayout,
  ProviderSnapshot,
} from './types';

export interface ProviderTabReading {
  id: string;
  label: string;
  /** Full reading with its unit, used for the tab's accessible name. */
  reading: string;
  /**
   * Rail display lines: numbers with at most a short symbol (`%`, `¥`, `$`).
   * Unit words stay in the dashboard card so the narrow rail keeps its digits.
   */
  lines: string[];
  available: boolean;
}

interface ReadingText {
  reading: string;
  lines: string[];
}

const MAX_RAIL_LINES = 2;

/** Caps multi-value readings and marks the hidden values, e.g. `$3.25 +1`. */
function railLines(values: string[]) {
  if (values.length <= MAX_RAIL_LINES) return values;
  const shown = values.slice(0, MAX_RAIL_LINES);
  shown[MAX_RAIL_LINES - 1] = `${shown[MAX_RAIL_LINES - 1]} +${values.length - MAX_RAIL_LINES}`;
  return shown;
}

function clampPercent(value: number) {
  return Math.min(100, Math.max(0, value));
}

function quotaReading(
  definition: MetricDefinition,
  snapshot: ProviderSnapshot,
  settings: AppSettings,
): ReadingText | null {
  const source = definition.source;
  if (source.kind !== 'quota' && source.kind !== 'quotaOrValue') return null;
  const quota = snapshot.quotas.find((item) => item.id === source.sourceId);
  if (!quota) return null;

  if (quota.format === 'count' && quota.usedValue !== null && quota.limitValue !== null) {
    const value =
      settings.usageDisplay === 'left'
        ? Math.max(0, quota.limitValue - quota.usedValue)
        : quota.usedValue;
    return {
      reading: formatMetricValue(value, 'count', 'row', quota.unit ?? undefined),
      lines: [formatMetricRailValue(value, 'count', quota.unit)],
    };
  }
  if (quota.format === 'dollars' && quota.usedValue !== null) {
    const value =
      settings.usageDisplay === 'left' && quota.limitValue !== null
        ? Math.max(0, quota.limitValue - quota.usedValue)
        : quota.usedValue;
    return {
      reading: formatMetricValue(value, 'dollars', 'row'),
      lines: [formatMetricRailValue(value, 'dollars')],
    };
  }

  const percent = clampPercent(quota.usedPercent);
  const displayed = settings.usageDisplay === 'left' ? 100 - percent : percent;
  const reading = `${Math.round(displayed)}%`;
  return { reading, lines: [reading] };
}

function valueReading(
  definition: MetricDefinition,
  snapshot: ProviderSnapshot,
): ReadingText | null {
  const source = definition.source;
  if (source.kind !== 'value' && source.kind !== 'quotaOrValue') return null;
  const metric = snapshot.valueMetrics.find((item) => item.id === source.sourceId);
  if (!metric || metric.values.length === 0) return null;
  return {
    reading: metric.values
      .map((value) => formatMetricValue(value.number, value.kind, 'row', value.label ?? undefined))
      .join(' · '),
    lines: railLines(
      metric.values.map((value) =>
        formatMetricRailValue(value.number, value.kind, value.label ?? undefined),
      ),
    ),
  };
}

function nearestCreditPackageReading(
  definition: MetricDefinition,
  snapshot: ProviderSnapshot,
): ReadingText | null {
  if (definition.source.kind !== 'nearestCreditPackage') return null;
  const package_ = snapshot.creditPackages.find((item) => !item.unlimited && item.remaining > 0);
  if (!package_) return null;
  return {
    reading: formatMetricValue(package_.remaining, 'count', 'row'),
    lines: [formatMetricRailValue(package_.remaining, 'count')],
  };
}

function statusReading(
  definition: MetricDefinition,
  snapshot: ProviderSnapshot,
): ReadingText | null {
  if (definition.source.kind !== 'status') return null;
  const sourceId = definition.source.sourceId;
  const text = snapshot.statusMetrics.find((item) => item.id === sourceId)?.text ?? null;
  return text === null ? null : { reading: text, lines: [text] };
}

function usageReading(
  definition: MetricDefinition,
  snapshot: ProviderSnapshot,
): ReadingText | null {
  if (definition.source.kind !== 'usage') return null;
  const period = snapshot.usage[definition.source.period];
  if (!period) return null;
  const tokens = formatMetricValue(period.tokens, 'count', 'row', t('units.tokens'));
  const railTokens = formatMetricRailValue(period.tokens, 'count', t('units.tokens'));
  if (period.estimatedCostUsd === null) return { reading: tokens, lines: [railTokens] };
  return {
    reading: `${formatMetricValue(period.estimatedCostUsd, 'dollars', 'row')} · ${tokens}`,
    lines: [formatMetricRailValue(period.estimatedCostUsd, 'dollars'), railTokens],
  };
}

function metricReading(
  definition: MetricDefinition,
  snapshot: ProviderSnapshot | null,
  settings: AppSettings,
): ReadingText | null {
  if (!snapshot) return null;
  return (
    quotaReading(definition, snapshot, settings) ??
    valueReading(definition, snapshot) ??
    nearestCreditPackageReading(definition, snapshot) ??
    statusReading(definition, snapshot) ??
    usageReading(definition, snapshot)
  );
}

export function pinnedMetricLayouts(
  provider: ProviderLayout,
  catalog: ProviderCatalogIndex,
): MetricLayout[] {
  return provider.metrics
    .filter((metric) => metric.pinned && Boolean(catalog.metric(metric.id)))
    .slice(0, 2);
}

export function providerTabReadings(
  provider: ProviderLayout,
  snapshot: ProviderSnapshot | null,
  settings: AppSettings,
  catalog: ProviderCatalogIndex,
): ProviderTabReading[] {
  return pinnedMetricLayouts(provider, catalog).map((layout) => {
    const definition = catalog.metric(layout.id)!;
    const text = metricReading(definition, snapshot, settings);
    return {
      id: layout.id,
      label: definition.label,
      reading: text?.reading ?? '--',
      lines: text?.lines ?? ['--'],
      available: text !== null,
    };
  });
}
