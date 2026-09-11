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
    if (event.key === 'ArrowRight') nextIndex = (index + 1) % tabs.length;
    else if (event.key === 'ArrowLeft') nextIndex = (index - 1 + tabs.length) % tabs.length;
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

<nav class="provider-tabs" aria-label={$tStore('dashboard.providerTabs')}>
  <div class="provider-tabs__scroller" role="tablist" aria-orientation="horizontal">
    {#each tabs as tab, index (tab.id ?? 'all')}
      <button
        class="provider-tab"
        class:provider-tab--active={tab.id === selectedProviderId}
        type="button"
        role="tab"
        aria-selected={tab.id === selectedProviderId}
        aria-label={tabLabel(tab)}
        tabindex={index === activeIndex ? 0 : -1}
        style={tab.provider
          ? `--provider-tab-accent: ${providerAccent(tab.provider.id)}`
          : undefined}
        bind:this={tabElements[index]}
        onclick={() => void onSelect(tab.id)}
        onkeydown={(event) => handleKeydown(event, index)}
      >
        <span class="provider-tab__identity">
          {#if tab.provider}
            <ProviderIcon providerId={tab.provider.id} size={16} />
          {:else}
            <Icon name="grid" size={16} strokeWidth={1.8} />
          {/if}
          <span class="provider-tab__name">{tab.name}</span>
        </span>
        {#if tab.readings.length > 0}
          <span class="provider-tab__readings" aria-hidden="true">
            {#each tab.readings as reading (reading.id)}
              <span
                class="provider-tab__reading"
                class:provider-tab__reading--empty={!reading.available}
              >
                <Icon name="star-filled" size={8} strokeWidth={1.5} />{reading.reading}
              </span>
            {/each}
          </span>
        {:else if tab.provider}
          <span class="provider-tab__readings provider-tab__readings--empty" aria-hidden="true">
            <span>--</span><span>--</span>
          </span>
        {/if}
        <span class="provider-tab__indicator" aria-hidden="true"></span>
      </button>
    {/each}
  </div>
</nav>

<style>
  .provider-tabs {
    min-width: 0;
    border-block-end: 1px solid var(--separator);
    background: var(--tray);
  }

  .provider-tabs__scroller {
    display: flex;
    min-width: 0;
    overflow-x: auto;
    overscroll-behavior-inline: contain;
    scrollbar-width: thin;
  }

  .provider-tab {
    position: relative;
    display: flex;
    min-width: 74px;
    min-height: 58px;
    flex: 0 0 auto;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 4px;
    padding: 7px 9px 8px;
    border: 0;
    color: var(--secondary);
    background: transparent;
    cursor: pointer;
    font-size: 10px;
    line-height: 12px;
    white-space: nowrap;
  }

  @media (hover: hover) {
    .provider-tab:hover {
      color: var(--text);
      background: var(--button-hover);
    }
  }

  .provider-tab:focus-visible {
    z-index: 1;
    outline: 2px solid var(--meter-fill);
    outline-offset: -2px;
  }

  .provider-tab--active {
    color: var(--text);
    background: var(--card);
  }

  .provider-tab__identity,
  .provider-tab__readings {
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .provider-tab__identity {
    max-width: 100%;
    gap: 5px;
  }

  .provider-tab__name {
    max-width: 82px;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .provider-tab__readings {
    gap: 7px;
    color: var(--provider-tab-accent, var(--secondary));
    font-size: 9px;
    font-variant-numeric: tabular-nums;
  }

  .provider-tab__reading {
    display: inline-flex;
    align-items: center;
    gap: 2px;
  }

  .provider-tab__reading--empty,
  .provider-tab__readings--empty {
    color: var(--tertiary);
  }

  .provider-tab__indicator {
    position: absolute;
    right: 9px;
    bottom: 0;
    left: 9px;
    height: 2px;
    border-radius: 2px 2px 0 0;
    background: transparent;
  }

  @supports (inset-inline: 0) {
    .provider-tab__indicator {
      inset-inline: 9px;
      inset-block-end: 0;
    }
  }

  .provider-tab--active .provider-tab__indicator {
    background: var(--provider-tab-accent, var(--meter-fill));
  }

  :root[data-density='compact'] .provider-tab {
    min-height: 52px;
    padding-block: 5px 6px;
  }

  @media (pointer: coarse) {
    .provider-tab {
      min-width: 82px;
      min-height: 64px;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .provider-tabs__scroller {
      scroll-behavior: auto;
    }
  }
</style>
