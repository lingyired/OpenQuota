import type { UsageUnit } from './types';

export type UsageAmountValue = {
  amount?: number | null;
  unit?: UsageUnit;
} & ({ tokens: number } | { totalTokens: number });

export function usageAmount(value: UsageAmountValue) {
  if (value.amount != null) return value.amount;
  return 'tokens' in value ? value.tokens : value.totalTokens;
}

export function usageUnit(value: UsageAmountValue): UsageUnit {
  return value.unit ?? 'tokens';
}
