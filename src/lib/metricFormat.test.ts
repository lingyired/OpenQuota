import { describe, expect, it } from 'vitest';
import {
  formatAmount,
  formatMetricNumber,
  formatMetricRailValue,
  formatMetricValue,
  formatSpendValue,
  railLineFontSize,
  railLineWidth,
  totalSpendRingCenter,
} from './metricFormat';

describe('shared metric formatting', () => {
  it('keeps row values compact and tooltip values at one decimal place', () => {
    expect(formatMetricNumber(2059.07, 'dollars', 'row')).toBe('$2.1K');
    expect(formatMetricNumber(2059.07, 'dollars', 'full')).toBe('$2,059.1');
    expect(formatMetricValue(1_506_025_363, 'count', 'row', 'tokens')).toBe('1.5B tokens');
    expect(formatMetricValue(1_506_025_363, 'count', 'full', 'tokens')).toBe(
      '1,506,025,363 tokens',
    );
  });

  it('formats total spend consistently across its surfaces', () => {
    expect(formatSpendValue(2059.07, 'cost')).toBe('$2.1K');
    expect(formatSpendValue(2059.07, 'cost', 'full')).toBe('$2,059.1');
    expect(totalSpendRingCenter(2059.07, 'cost')).toEqual({
      primary: '$2.1K',
      unit: 'dollars',
    });
    expect(totalSpendRingCenter(461_800_000, 'tokens')).toEqual({
      primary: '461.8',
      unit: 'million',
    });
  });

  it('reads amounts and credits at one decimal place and drops trailing zeros', () => {
    expect(formatMetricNumber(3.25, 'dollars', 'full')).toBe('$3.3');
    expect(formatMetricNumber(12, 'dollars', 'full')).toBe('$12');
    expect(formatAmount(71.38)).toBe('71.4');
    expect(formatAmount(2.0)).toBe('2');
  });

  it('marks credits with ✦ instead of spelling the unit out', () => {
    expect(formatMetricValue(71.38, 'count', 'row', 'credits')).toBe('✦71.4');
    expect(formatMetricValue(1019.42, 'count', 'row', 'credits')).toBe('✦1K');
    expect(formatMetricValue(30_000, 'count', 'full', '积分')).toBe('✦30,000');
    expect(formatMetricRailValue(821, 'count', 'credits')).toBe('✦821');
    // A count unit with no marker of its own keeps both its word and its bare rail line.
    expect(formatMetricValue(71.38, 'count', 'row', 'requests')).toBe('71.4 requests');
    expect(formatMetricRailValue(71.38, 'count', 'requests')).toBe('71.4');
  });

  it('keeps currency codes in rows and narrow symbols in rails', () => {
    expect(formatMetricValue(110, 'currency', 'row', 'CNY')).toBe('110 CNY');
    expect(formatMetricValue(3.25, 'currency', 'row', 'USD')).toBe('3.3 USD');
    expect(formatMetricRailValue(110, 'currency', 'CNY')).toBe('¥110');
    expect(formatMetricRailValue(3.25, 'currency', 'USD')).toBe('$3.3');
    expect(formatMetricValue(2059.07, 'currency', 'row', 'CNY')).toBe('2.1K CNY');
  });

  it('never rounds a real balance away to zero', () => {
    expect(formatMetricValue(0.3003, 'currency', 'row', 'CNY')).toBe('0.3 CNY');
    expect(formatMetricValue(0.04, 'currency', 'row', 'CNY')).toBe('0.04 CNY');
    expect(formatMetricValue(0.0004, 'currency', 'row', 'USD')).toBe('0.0004 USD');
    expect(formatMetricRailValue(0.0004, 'currency', 'USD')).toBe('$0.0004');
    expect(formatAmount(0)).toBe('0');
  });
});

describe('provider rail line fitting', () => {
  it('keeps glanceable readings at the full rail size', () => {
    expect(railLineFontSize('36%')).toBe(11);
    expect(railLineFontSize('¥110')).toBe(11);
    expect(railLineFontSize('$3.84')).toBe(11);
    expect(railLineFontSize('2.1M')).toBe(11);
    expect(railLineFontSize('¥1.2K')).toBe(11);
    expect(railLineFontSize('✦821')).toBe(11);
  });

  it('shrinks longer readings instead of truncating them', () => {
    for (const text of ['$999.99', '$3.3 +1', '¥1,234.6', '✦1,234.6']) {
      const size = railLineFontSize(text);
      expect(size).toBeLessThan(11);
      expect(size).toBeGreaterThanOrEqual(8.5);
      expect(railLineWidth(text, size)).toBeLessThanOrEqual(44);
    }
  });

  it('never grows past the rail size and never shrinks below the floor', () => {
    expect(railLineFontSize('')).toBe(11);
    expect(railLineFontSize('$')).toBe(11);
    expect(railLineFontSize('Unavailable')).toBe(8.5);
  });

  it('never gives a longer reading a larger size than a shorter one', () => {
    expect(railLineFontSize('$999.99')).toBeLessThanOrEqual(railLineFontSize('$99.99'));
    expect(railLineFontSize('¥1,234.6')).toBeLessThanOrEqual(railLineFontSize('¥1.2K'));
    expect(railLineFontSize('¥11,234.6')).toBeLessThanOrEqual(railLineFontSize('¥1,234.6'));
  });
});
