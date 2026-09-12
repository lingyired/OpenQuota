<script lang="ts">
  import type { CreditPackage } from './types';

  interface Props {
    packages: CreditPackage[];
    expanded?: boolean;
    label?: string;
  }

  let { packages, expanded = false, label = '可用积分包' }: Props = $props();
  const visiblePackages = $derived(
    packages
      .filter((item) => item.unlimited || item.remaining > 0)
      .slice(0, expanded ? undefined : 3),
  );

  const numberFormatter = new Intl.NumberFormat('zh-CN', { maximumFractionDigits: 2 });
  const formatCredits = (value: number) =>
    Number.isFinite(value) ? numberFormatter.format(value) : '—';
  const fillPercent = (item: CreditPackage) =>
    item.total > 0 ? Math.min(100, Math.max(0, (item.remaining / item.total) * 100)) : 0;
  const formatExpiry = (value: string | null) => {
    if (!value) return '—';
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return '—';
    return date.toLocaleDateString('zh-CN', {
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
    });
  };
</script>

{#if visiblePackages.length > 0}
  <section class="metric credit-packages" aria-label={label}>
    <div class="metric__heading"><h2>{label}</h2></div>
    <div class="credit-packages__items">
      {#each visiblePackages as creditPackage, index (`${creditPackage.code}:${creditPackage.expiresAt ?? 'none'}:${index}`)}
        <article class="credit-package">
          <div class="credit-package__header">
            <strong title={creditPackage.name || creditPackage.code}
              >{creditPackage.name || creditPackage.code}</strong
            >
            {#if creditPackage.unlimited}
              <span>无限</span>
            {:else}
              <span
                >{formatCredits(creditPackage.remaining)} / {formatCredits(
                  creditPackage.total,
                )}</span
              >
            {/if}
          </div>
          {#if !creditPackage.unlimited}
            <div class="meter-shell">
              <div
                class="meter"
                role="progressbar"
                aria-label={`${creditPackage.name || creditPackage.code} 剩余积分`}
                aria-valuemin="0"
                aria-valuemax="100"
                aria-valuenow={fillPercent(creditPackage)}
              >
                <span
                  class="meter__fill meter__fill--visible"
                  style={`--fill-percent: ${fillPercent(creditPackage)}%`}
                ></span>
              </div>
            </div>
          {/if}
          <div class="credit-package__meta">
            <span>剩余</span>
            <span>到期 {formatExpiry(creditPackage.expiresAt)}</span>
          </div>
        </article>
      {/each}
    </div>
  </section>
{/if}

<style>
  .credit-packages {
    padding: 10px 14px;
  }

  .credit-packages__items {
    display: grid;
    gap: 5px;
  }

  .credit-package {
    min-width: 0;
    padding: 4px 0;
    border-radius: 6px;
  }

  .credit-package__header,
  .credit-package__meta {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 8px;
  }

  .credit-package__header strong {
    min-width: 0;
    overflow: hidden;
    color: var(--text);
    font-size: 11px;
    font-weight: 650;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .credit-package__header span {
    flex: 0 0 auto;
    color: var(--text);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
    font-weight: 650;
  }

  .credit-package__meta {
    margin-top: 2px;
    color: var(--secondary);
    font-size: 10px;
    line-height: 13px;
  }

  .credit-package .meter-shell {
    margin-top: 4px;
  }

  :global(html[data-density='compact']) .credit-packages {
    padding-block: 4px;
  }
</style>
