import { describe, expect, it } from 'vitest';
import {
  formatMetricNumber,
  formatMetricValue,
  formatSpendValue,
  railLineFontSize,
  railLineWidth,
  totalSpendRingCenter,
} from './metricFormat';

describe('shared metric formatting', () => {
  it('keeps row values compact and tooltip values exact', () => {
    expect(formatMetricNumber(2059.07, 'dollars', 'row')).toBe('$2.1K');
    expect(formatMetricNumber(2059.07, 'dollars', 'full')).toBe('$2,059.07');
    expect(formatMetricValue(1_506_025_363, 'count', 'row', 'tokens')).toBe('1.5B tokens');
    expect(formatMetricValue(1_506_025_363, 'count', 'full', 'tokens')).toBe(
      '1,506,025,363 tokens',
    );
  });

  it('formats total spend consistently across its surfaces', () => {
    expect(formatSpendValue(2059.07, 'cost')).toBe('$2.1K');
    expect(formatSpendValue(2059.07, 'cost', 'full')).toBe('$2,059.07');
    expect(totalSpendRingCenter(2059.07, 'cost')).toEqual({
      primary: '$2.1K',
      unit: 'dollars',
    });
    expect(totalSpendRingCenter(461_800_000, 'tokens')).toEqual({
      primary: '461.8',
      unit: 'million',
    });
  });
});

describe('provider rail line fitting', () => {
  it('keeps glanceable readings at the full rail size', () => {
    expect(railLineFontSize('36%')).toBe(11);
    expect(railLineFontSize('¥110')).toBe(11);
    expect(railLineFontSize('$3.84')).toBe(11);
    expect(railLineFontSize('2.1M')).toBe(11);
    expect(railLineFontSize('¥1.2K')).toBe(11);
  });

  it('shrinks longer readings instead of truncating them', () => {
    for (const text of ['$999.99', '$3.3 +1', '¥1,234.6']) {
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
