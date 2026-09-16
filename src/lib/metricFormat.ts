import { t } from './i18n';
import type { AppSettings } from './types';

export type MetricNumberKind = 'percent' | 'dollars' | 'count';
export type MetricNumberStyle = 'tray' | 'row' | 'full';

const compactFormatter = new Intl.NumberFormat('en-US', {
  notation: 'compact',
  maximumFractionDigits: 1,
});
const rowNumberFormatter = new Intl.NumberFormat('en-US', {
  minimumFractionDigits: 0,
  maximumFractionDigits: 1,
});
const fullNumberFormatter = new Intl.NumberFormat('en-US', {
  minimumFractionDigits: 0,
  maximumFractionDigits: 1,
});
const currencyFormatter = new Intl.NumberFormat('en-US', {
  style: 'currency',
  currency: 'USD',
  minimumFractionDigits: 2,
  maximumFractionDigits: 2,
});
const wholeDollarFormatter = new Intl.NumberFormat('en-US', {
  style: 'currency',
  currency: 'USD',
  minimumFractionDigits: 0,
  maximumFractionDigits: 0,
});
const currencySymbolFormatters = new Map<string, Intl.NumberFormat | null>();

function createCurrencyFormatter(code: string) {
  try {
    return new Intl.NumberFormat('en-US', {
      style: 'currency',
      currency: code,
      currencyDisplay: 'narrowSymbol',
      minimumFractionDigits: 0,
      maximumFractionDigits: 1,
    });
  } catch {
    return null;
  }
}

function currencySymbol(code: string) {
  if (!currencySymbolFormatters.has(code)) {
    currencySymbolFormatters.set(code, createCurrencyFormatter(code));
  }
  const formatter = currencySymbolFormatters.get(code);
  return formatter
    ? formatter.formatToParts(0).find((part) => part.type === 'currency')?.value
    : null;
}

export function formatMetricNumber(
  value: number,
  kind: MetricNumberKind,
  style: MetricNumberStyle,
) {
  if (!Number.isFinite(value)) return '—';
  if (kind === 'percent') return `${Math.round(Math.min(100, Math.max(0, value)))}%`;
  if (kind === 'dollars') {
    if (Math.abs(value) >= 1000 && style !== 'full') {
      return `$${compactFormatter.format(value)}`;
    }
    return style === 'tray' ? wholeDollarFormatter.format(value) : currencyFormatter.format(value);
  }
  if (style !== 'full' && Math.abs(value) >= 1000) return compactFormatter.format(value);
  return (style === 'full' ? fullNumberFormatter : rowNumberFormatter).format(value);
}

export function formatMetricValue(
  value: number,
  kind: MetricNumberKind,
  style: MetricNumberStyle,
  label?: string,
) {
  const formatted = formatMetricNumber(value, kind, style);
  return label ? `${formatted} ${label}` : formatted;
}

/**
 * Compact variant for the provider rail, where space is measured in a few
 * characters. Percent and dollar readings stay as they are, currency codes
 * collapse to their narrow symbol (`CNY` → `¥`), and word units (`tokens`,
 * `credits`, account names) are dropped because the dashboard card next to the
 * rail repeats the reading with its unit.
 */
export function formatMetricRailValue(
  value: number,
  kind: MetricNumberKind,
  label?: string | null,
) {
  const formatted = formatMetricNumber(value, kind, 'row');
  if (kind !== 'count' || !label || !/^[A-Za-z]{3}$/.test(label)) return formatted;
  const symbol = currencySymbol(label.toUpperCase());
  return symbol ? `${symbol}${formatted}` : formatted;
}

/** Digit column of the rail: 92px rail − 12px insets − 8px tab padding − 22px icon − 6px gap. */
export const RAIL_LINE_WIDTH_PX = 44;
const RAIL_LINE_MAX_FONT_SIZE = 11;
const RAIL_LINE_MIN_FONT_SIZE = 8.5;
const RAIL_LETTER_SPACING_PX = 0.3;
const RAIL_FIT_WIDTH_PX = RAIL_LINE_WIDTH_PX * 0.96;

/**
 * Advance widths in `em` for the glyphs the rail renders, anchored on the
 * tabular figure width of `system-ui` and rounded up for symbols so the fit
 * stays conservative.
 */
const railGlyphWidthEm: Record<string, number> = {
  ' ': 0.35,
  '.': 0.35,
  ',': 0.35,
  '+': 0.7,
  '-': 0.45,
  '%': 0.95,
  $: 0.72,
  '¥': 0.85,
  '€': 0.72,
  '£': 0.72,
  '₩': 0.95,
};
const RAIL_DIGIT_WIDTH_EM = 0.7;
const RAIL_LETTER_WIDTH_EM = 0.72;
const RAIL_OTHER_WIDTH_EM = 1;

function railGlyph(character: string) {
  const known = railGlyphWidthEm[character];
  if (known !== undefined) return known;
  if (character >= '0' && character <= '9') return RAIL_DIGIT_WIDTH_EM;
  if (character.toLowerCase() !== character.toUpperCase()) return RAIL_LETTER_WIDTH_EM;
  return RAIL_OTHER_WIDTH_EM;
}

/** Estimated rendered width of a rail line, including the rail's letter spacing. */
export function railLineWidth(text: string, fontSize: number) {
  const characters = [...text];
  const glyphs = characters.reduce((total, character) => total + railGlyph(character), 0);
  return glyphs * fontSize - RAIL_LETTER_SPACING_PX * characters.length;
}

/**
 * Rail readings shrink instead of truncating: longer lines step down in half
 * pixels until they fit the digit column, then stop at a legible floor where
 * the remaining overflow falls back to the rail's ellipsis.
 */
export function railLineFontSize(text: string, availableWidth = RAIL_FIT_WIDTH_PX) {
  if (railLineWidth(text, RAIL_LINE_MAX_FONT_SIZE) <= availableWidth) {
    return RAIL_LINE_MAX_FONT_SIZE;
  }
  const characters = [...text];
  const glyphs = characters.reduce((total, character) => total + railGlyph(character), 0);
  if (glyphs === 0) return RAIL_LINE_MAX_FONT_SIZE;
  const fitted = (availableWidth + RAIL_LETTER_SPACING_PX * characters.length) / glyphs;
  return Math.min(
    RAIL_LINE_MAX_FONT_SIZE,
    Math.max(RAIL_LINE_MIN_FONT_SIZE, Math.round(fitted * 2) / 2),
  );
}

export function formatSpendValue(
  value: number,
  metric: AppSettings['totalSpendMetric'],
  style: MetricNumberStyle = 'row',
) {
  if (metric === 'tokens') return formatMetricNumber(value, 'count', style);
  const dollars = formatMetricNumber(value, 'dollars', style);
  return metric === 'costPerMillion' ? `${dollars}/${t('units.mtok')}` : dollars;
}

export function totalSpendRingCenter(value: number, metric: AppSettings['totalSpendMetric']) {
  if (metric === 'cost') {
    return { primary: formatMetricNumber(value, 'dollars', 'tray'), unit: t('units.dollars') };
  }
  if (metric === 'costPerMillion') {
    return { primary: formatMetricNumber(value, 'dollars', 'row'), unit: t('units.mtok') };
  }
  const magnitude = Math.abs(value);
  if (magnitude >= 1_000_000_000) {
    return { primary: rowNumberFormatter.format(value / 1_000_000_000), unit: t('units.billion') };
  }
  if (magnitude >= 1_000_000) {
    return { primary: rowNumberFormatter.format(value / 1_000_000), unit: t('units.million') };
  }
  if (magnitude >= 1_000) {
    return { primary: rowNumberFormatter.format(value / 1_000), unit: t('units.thousand') };
  }
  return { primary: rowNumberFormatter.format(value), unit: t('units.tokens') };
}
