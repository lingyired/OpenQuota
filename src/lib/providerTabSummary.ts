import { formatMetricValue } from './metricFormat';
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
  reading: string;
  available: boolean;
}

function clampPercent(value: number) {
  return Math.min(100, Math.max(0, value));
}

function quotaReading(
  definition: MetricDefinition,
  snapshot: ProviderSnapshot,
  settings: AppSettings,
): string | null {
  const source = definition.source;
  if (source.kind !== 'quota' && source.kind !== 'quotaOrValue') return null;
  const quota = snapshot.quotas.find((item) => item.id === source.sourceId);
  if (!quota) return null;

  if (quota.format === 'count' && quota.usedValue !== null && quota.limitValue !== null) {
    const value =
      settings.usageDisplay === 'left'
        ? Math.max(0, quota.limitValue - quota.usedValue)
        : quota.usedValue;
    return formatMetricValue(value, 'count', 'row', quota.unit ?? undefined);
  }
  if (quota.format === 'dollars' && quota.usedValue !== null) {
    const value =
      settings.usageDisplay === 'left' && quota.limitValue !== null
        ? Math.max(0, quota.limitValue - quota.usedValue)
        : quota.usedValue;
    return formatMetricValue(value, 'dollars', 'row');
  }

  const percent = clampPercent(quota.usedPercent);
  const displayed = settings.usageDisplay === 'left' ? 100 - percent : percent;
  return `${Math.round(displayed)}%`;
}

function valueReading(definition: MetricDefinition, snapshot: ProviderSnapshot): string | null {
  const source = definition.source;
  if (source.kind !== 'value' && source.kind !== 'quotaOrValue') return null;
  const metric = snapshot.valueMetrics.find((item) => item.id === source.sourceId);
  if (!metric || metric.values.length === 0) return null;
  return metric.values
    .map((value) => formatMetricValue(value.number, value.kind, 'row', value.label ?? undefined))
    .join(' · ');
}

function nearestCreditPackageReading(
  definition: MetricDefinition,
  snapshot: ProviderSnapshot,
): string | null {
  if (definition.source.kind !== 'nearestCreditPackage') return null;
  const package_ = snapshot.creditPackages.find((item) => !item.unlimited && item.remaining > 0);
  return package_ ? formatMetricValue(package_.remaining, 'count', 'row') : null;
}

function statusReading(definition: MetricDefinition, snapshot: ProviderSnapshot): string | null {
  if (definition.source.kind !== 'status') return null;
  const sourceId = definition.source.sourceId;
  return snapshot.statusMetrics.find((item) => item.id === sourceId)?.text ?? null;
}

function usageReading(definition: MetricDefinition, snapshot: ProviderSnapshot): string | null {
  if (definition.source.kind !== 'usage') return null;
  const period = snapshot.usage[definition.source.period];
  if (!period) return null;
  const tokens = formatMetricValue(period.tokens, 'count', 'row', t('units.tokens'));
  if (period.estimatedCostUsd === null) return tokens;
  return `${formatMetricValue(period.estimatedCostUsd, 'dollars', 'row')} · ${tokens}`;
}

function metricReading(
  definition: MetricDefinition,
  snapshot: ProviderSnapshot | null,
  settings: AppSettings,
): string | null {
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
    const reading = metricReading(definition, snapshot, settings);
    return {
      id: layout.id,
      label: definition.label,
      reading: reading ?? '--',
      available: reading !== null,
    };
  });
}
