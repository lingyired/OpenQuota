import { describe, expect, it } from 'vitest';
import type { ModelUsageEntry } from './types';
import { usageAmount, usageUnit } from './usageAmount';

describe('usageAmount', () => {
  it('uses normalized credit amounts for model entries without a unit field', () => {
    const model: ModelUsageEntry = {
      model: 'workbuddy-model',
      totalTokens: 0,
      amount: 12.5,
      costUsd: null,
    };

    expect(usageAmount(model)).toBe(12.5);
  });

  it('keeps legacy token fields as the fallback', () => {
    expect(usageAmount({ tokens: 42 })).toBe(42);
    expect(usageAmount({ totalTokens: 84 })).toBe(84);
  });

  it('defaults omitted units to tokens', () => {
    expect(usageUnit({ tokens: 42 })).toBe('tokens');
    expect(usageUnit({ tokens: 42, unit: 'credits' })).toBe('credits');
  });
});
