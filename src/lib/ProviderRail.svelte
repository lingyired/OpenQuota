<script lang="ts">
  import { locale } from 'svelte-i18n';
  import Icon from './Icon.svelte';
  import { t, tBackend, tStore } from './i18n';
  import { providerIconColor } from './providerIconPaths';
  import ProviderIcon from './ProviderIcon.svelte';
  import { providerTabReadings, type ProviderTabReading } from './providerTabSummary';
  import type { ProviderCatalogIndex } from './metrics';
  import type { AppSettings, ProviderLayout, UsageViewState } from './types';

  interface Props {
    viewState: UsageViewState;
    settings: AppSettings;
    catalog: ProviderCatalogIndex;
    selectedProviderId: string | null;
    onSelect: (providerId: string | null) => void | Promise<void>;
  }

  type TabId = string | null;
  interface TabItem {
    id: TabId;
    name: string;
    provider: ProviderLayout | null;
    readings: ProviderTabReading[];
  }

  let { viewState, settings, catalog, selectedProviderId, onSelect }: Props = $props();
  let tabElements = $state<HTMLButtonElement[]>([]);
  const currentLocale = $derived($locale);
  const enabledProviders = $derived(
    settings.providers.filter((provider) => provider.enabled && catalog.provider(provider.id)),
  );
  const tabs = $derived.by(() => {
    void currentLocale;
    return [
      { id: null, name: t('dashboard.all'), provider: null, readings: [] },
      ...enabledProviders.map((provider) => ({
        id: provider.id,
        name: catalog.displayName(provider.id, settings.providerNames),
        provider,
        readings: providerTabReadings(
          provider,
          viewState.providers[provider.id]?.snapshot ?? null,
          settings,
          catalog,
        ),
      })),
    ] satisfies TabItem[];
  });
  const activeIndex = $derived(
    Math.max(
      0,
      tabs.findIndex((tab) => tab.id === selectedProviderId),
    ),
  );

  $effect(() => {
    const tab = tabElements[activeIndex];
    tab?.scrollIntoView?.({ block: 'nearest', inline: 'nearest' });
  });

  function providerAccent(providerId: string) {
    return providerIconColor(providerId) ?? 'var(--provider)';
  }

  function tabLabel(tab: TabItem) {
    void currentLocale;
    if (!tab.provider) return t('dashboard.all');
    const readings = tab.readings.length
      ? tab.readings.map((reading) => `${tBackend(reading.label)}: ${reading.reading}`).join(', ')
      : t('metric.noData');
    return `${tab.name}: ${readings}`;
  }

  function handleKeydown(event: KeyboardEvent, index: number) {
    let nextIndex: number | null = null;
    if (event.key === 'ArrowDown') nextIndex = (index + 1) % tabs.length;
    else if (event.key === 'ArrowUp') nextIndex = (index - 1 + tabs.length) % tabs.length;
    else if (event.key === 'Home') nextIndex = 0;
    else if (event.key === 'End') nextIndex = tabs.length - 1;

    if (nextIndex !== null) {
      event.preventDefault();
      tabElements[nextIndex]?.focus();
      void onSelect(tabs[nextIndex].id);
      return;
    }
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      void onSelect(tabs[index].id);
    }
  }
</script>

<nav class="provider-rail" aria-label={$tStore('dashboard.providerTabs')}>
  <div class="provider-rail__scroller" role="tablist" aria-orientation="vertical">
    {#each tabs as tab, index (tab.id ?? 'all')}
      <button
        class="provider-rail__tab"
        class:provider-rail__tab--active={tab.id === selectedProviderId}
        type="button"
        role="tab"
        aria-selected={tab.id === selectedProviderId}
        aria-label={tabLabel(tab)}
        tabindex={index === activeIndex ? 0 : -1}
        style={tab.provider
          ? `--provider-rail-accent: ${providerAccent(tab.provider.id)}`
          : undefined}
        bind:this={tabElements[index]}
        onclick={() => void onSelect(tab.id)}
        onkeydown={(event) => handleKeydown(event, index)}
      >
        <span class="provider-rail__icon">
          {#if tab.provider}
            <ProviderIcon providerId={tab.provider.id} size={22} />
          {:else}
            <Icon name="grid" size={22} strokeWidth={1.8} />
          {/if}
        </span>
        <span class="provider-rail__values" aria-hidden="true">
          {#if tab.provider}
            {#if tab.readings.length > 0}
              {#each tab.readings as reading (reading.id)}
                <span
                  class="provider-rail__reading"
                  class:provider-rail__reading--empty={!reading.available}>{reading.reading}</span
                >
              {/each}
            {:else}
              <span class="provider-rail__reading provider-rail__reading--empty">--</span>
            {/if}
          {:else}
            <span class="provider-rail__all">ALL</span>
          {/if}
        </span>
      </button>
    {/each}
  </div>
</nav>

<style>
  .provider-rail {
    width: 80px;
    min-width: 80px;
    min-height: 0;
    border-inline-end: 1px solid var(--separator);
    background: var(--tray);
    overflow: hidden;
  }

  .provider-rail__scroller {
    display: flex;
    height: 100%;
    min-height: 0;
    flex-direction: column;
    gap: 4px;
    padding: 7px 6px 10px;
    overflow-y: auto;
    overscroll-behavior: contain;
    scrollbar-color: var(--separator) transparent;
    scrollbar-width: thin;
  }

  .provider-rail__tab {
    position: relative;
    display: flex;
    width: 100%;
    height: 60px;
    flex: 0 0 60px;
    flex-direction: row;
    align-items: center;
    justify-content: flex-start;
    gap: 6px;
    padding: 5px 4px;
    border: 0;
    border-radius: 10px;
    color: var(--secondary);
    background: transparent;
    cursor: pointer;
    font: inherit;
  }

  @media (hover: hover) {
    .provider-rail__tab:hover {
      color: var(--text);
      background: var(--button-hover);
    }
  }

  .provider-rail__tab:focus,
  .provider-rail__tab:focus-visible {
    outline: none;
  }

  .provider-rail__tab--active {
    color: var(--text);
    background: color-mix(in srgb, var(--text) 7%, transparent);
  }

  .provider-rail__tab--active::before {
    position: absolute;
    inset-block: 10px;
    inset-inline-start: -5px;
    width: 2px;
    border-radius: 2px;
    background: var(--provider-rail-accent, var(--meter-fill));
    content: '';
  }

  .provider-rail__icon,
  .provider-rail__values {
    display: flex;
    align-items: center;
    justify-content: center;
    min-width: 0;
    max-width: 100%;
  }

  .provider-rail__icon {
    flex: 0 0 22px;
    height: 22px;
  }

  .provider-rail__values {
    flex: 1 1 auto;
    flex-direction: column;
    align-items: flex-start;
    gap: 0;
    color: var(--provider-rail-accent, var(--secondary));
    font-size: 10px;
    font-variant-numeric: tabular-nums;
    line-height: 12px;
    letter-spacing: -0.3px;
  }

  .provider-rail__reading {
    display: block;
    width: 100%;
    min-width: 0;
    overflow: hidden;
    text-align: left;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .provider-rail__reading--empty {
    color: var(--tertiary);
  }

  .provider-rail__all {
    color: var(--secondary);
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.04em;
  }

  @media (pointer: coarse) {
    .provider-rail__tab {
      height: 64px;
      flex-basis: 64px;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .provider-rail__scroller {
      scroll-behavior: auto;
    }
  }
</style>
