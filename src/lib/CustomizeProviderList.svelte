<script lang="ts">
  import { flip } from 'svelte/animate';
  import { tBackendStore, tStore } from './i18n';
  import type { AppSettings, ProviderLayout } from './types';
  import type { ProviderCatalogIndex } from './metrics';
  import Icon from './Icon.svelte';
  import ProviderIcon from './ProviderIcon.svelte';
  import { reorderFlip } from './motion';
  import { pointerReorder } from './pointerReorder';

  interface Props {
    settings: AppSettings;
    catalog: ProviderCatalogIndex;
    onOpen: (providerId: string) => void;
    onChange: (settings: AppSettings) => void;
    onReorderStart: () => void;
    onReorderEnd: (moved: boolean, cancelled?: boolean) => void;
    onSettings: () => void;
    reducedMotion: boolean;
    workspace?: boolean;
    selectedProviderId?: string | null;
    generalSelected?: boolean;
    providerInstanceCount?: number;
    providerInstanceFailures?: string[];
    onSelect?: (providerId: string) => void;
    onGeneralSettings?: () => void;
  }
  let {
    settings,
    catalog,
    onOpen,
    onChange,
    onReorderStart,
    onReorderEnd,
    onSettings,
    reducedMotion,
    workspace = false,
    selectedProviderId = null,
    generalSelected = false,
    providerInstanceCount = 0,
    providerInstanceFailures = [],
    onSelect,
    onGeneralSettings,
  }: Props = $props();
  const providerDisplayName = (id: string) => catalog.displayName(id, settings.providerNames);
  function updateProvider(provider: ProviderLayout) {
    onChange({
      ...settings,
      providers: settings.providers.map((item) => (item.id === provider.id ? provider : item)),
    });
  }
  function reorder(draggedId: string, targetId: string) {
    if (draggedId === targetId) return;
    const enabled = settings.providers.filter((provider) => provider.enabled);
    const from = enabled.findIndex((provider) => provider.id === draggedId);
    const to = enabled.findIndex((provider) => provider.id === targetId);
    if (from < 0 || to < 0) return;
    const [provider] = enabled.splice(from, 1);
    enabled.splice(to, 0, provider);
    onChange({
      ...settings,
      providers: [...enabled, ...settings.providers.filter((provider) => !provider.enabled)],
    });
  }
</script>

<section
  class="screen customize-screen"
  aria-label={workspace ? undefined : $tStore('customize.title')}
>
  {#if workspace}
    <button
      class="workspace-general-entry"
      class:workspace-general-entry--selected={generalSelected}
      type="button"
      aria-current={generalSelected ? 'true' : undefined}
      onclick={onGeneralSettings}
    >
      <Icon name="gear" size={18} />
      <span>
        <b>{$tStore('settings.general')}</b>
        <small>{$tStore('customize.settingsDesc')}</small>
      </span>
      <Icon name="chevron-right" size={13} strokeWidth={2.2} />
    </button>
    <div class="workspace-provider-heading">
      <h2>{$tStore('settings.providers')}</h2>
      <span aria-live="polite"
        >{$tStore('customize.nativeInstanceCount', { count: providerInstanceCount })}</span
      >
    </div>
    {#each providerInstanceFailures as failure (failure)}
      <p class="workspace-provider-failure" role="status">{failure}</p>
    {/each}
  {/if}
  <div class="customize-list" role="list">
    {#each settings.providers.filter( (provider) => catalog.provider(provider.id) ) as provider (provider.id)}
      <div
        role="listitem"
        class:inactive={!provider.enabled}
        class:provider-list-row--workspace={workspace}
        class:provider-list-row--selected={workspace && selectedProviderId === provider.id}
        class="provider-list-row"
        data-reorder-group={provider.enabled ? 'customize-providers' : undefined}
        data-reorder-id={provider.enabled ? provider.id : undefined}
        use:pointerReorder={{
          id: provider.id,
          group: 'customize-providers',
          label: providerDisplayName(provider.id),
          disabled: !provider.enabled,
          gripOnly: true,
          touchGripOnly: true,
          onReorder: (targetId) => reorder(provider.id, targetId),
          onStart: onReorderStart,
          onEnd: onReorderEnd,
        }}
        animate:flip={reorderFlip(reducedMotion)}
      >
        <span
          class="reorder-grip"
          data-reorder-handle
          data-reorder-touch-handle
          role="button"
          tabindex={provider.enabled ? 0 : undefined}
          aria-label={$tStore('customize.moveProvider', { name: providerDisplayName(provider.id) })}
          aria-describedby="reorder-instructions"
          aria-keyshortcuts="Alt+ArrowUp Alt+ArrowDown"
          ><Icon name="grip-lines" size={16} strokeWidth={2} /></span
        >
        <button
          class="provider-list-main"
          type="button"
          aria-label={workspace ? providerDisplayName(provider.id) : undefined}
          aria-current={workspace && selectedProviderId === provider.id ? 'true' : undefined}
          onclick={() => (workspace ? onSelect?.(provider.id) : onOpen(provider.id))}
        >
          <ProviderIcon providerId={provider.id} />
          <span>
            <b>{providerDisplayName(provider.id)}</b>
            <small>{$tStore('customize.metricCount', { count: provider.metrics.length })}</small>
            {#if workspace && settings.taskbandProviders[provider.id]?.enabled === false}
              <small>{$tStore('customize.taskbar')} · {$tBackendStore('common.disabled')}</small>
            {/if}
          </span>
        </button>
        <label class="switch"
          ><input
            aria-label={$tStore('customize.enableProvider', { id: provider.id })}
            type="checkbox"
            checked={provider.enabled}
            onchange={(event) => {
              const enabled = event.currentTarget.checked;
              updateProvider({ ...provider, enabled });
            }}
          /><span></span></label
        >
        <button
          class="chevron"
          type="button"
          aria-label={$tStore('customize.customizeProvider', { id: provider.id })}
          onclick={() => (workspace ? onSelect?.(provider.id) : onOpen(provider.id))}
          ><Icon name="chevron-right" size={13} strokeWidth={2.2} /></button
        >
      </div>
    {/each}
  </div>
  {#if !workspace}
    <button
      class="screen-cross-link"
      type="button"
      aria-label={$tStore('customize.settings')}
      onclick={onSettings}
    >
      <Icon name="gear" size={17} />
      <span>
        <b>{$tStore('customize.settings')}</b>
        <small>{$tStore('customize.settingsDesc')}</small>
      </span>
      <Icon name="chevron-right" size={13} strokeWidth={2.2} />
    </button>
  {/if}
</section>

<style>
  .workspace-general-entry {
    display: flex;
    width: 100%;
    min-height: 52px;
    align-items: center;
    gap: 10px;
    padding-block: 9px;
    padding-inline: 9px 12px;
    border: 0;
    border-inline-start: 3px solid transparent;
    border-radius: 11px;
    color: var(--text);
    background: var(--card);
    font: inherit;
    text-align: start;
    cursor: pointer;
  }

  .workspace-general-entry > span {
    display: flex;
    min-width: 0;
    flex: 1;
    flex-direction: column;
  }

  .workspace-general-entry b {
    font-size: 14px;
    font-weight: 600;
  }

  .workspace-general-entry small {
    color: var(--secondary);
    font-size: 11px;
  }

  .workspace-general-entry:hover {
    background: var(--card-hover);
  }

  .workspace-general-entry--selected {
    border-inline-start-color: var(--meter-fill);
    background: color-mix(in srgb, var(--meter-fill) 12%, var(--card));
  }

  .workspace-general-entry:focus-visible {
    outline: 2px solid var(--meter-fill);
    outline-offset: 2px;
  }

  .workspace-provider-heading {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 4px 8px;
    padding: 18px 8px 7px;
    color: var(--secondary);
  }

  .workspace-provider-heading h2 {
    margin: 0;
    font-size: 11px;
    font-weight: 650;
  }

  .workspace-provider-heading span,
  .workspace-provider-failure {
    font-size: 10px;
    line-height: 1.4;
  }

  .workspace-provider-failure {
    margin: 0 8px 8px;
    color: var(--error);
  }

  :global {
    .provider-list-main[aria-current='true'] {
      color: var(--text);
    }

    .provider-list-main:focus-visible {
      border-radius: 7px;
      outline: 2px solid var(--meter-fill);
      outline-offset: 2px;
    }

    .provider-list-row {
      display: flex;
      min-height: 52px;
      align-items: center;
      gap: 5px;
      padding: 5px 7px;
      border-top: 1px solid var(--separator);
    }

    .provider-list-row:first-child {
      border-top: 0;
    }

    .provider-list-row.inactive {
      opacity: 0.55;
    }

    .provider-list-row.provider-list-row--workspace {
      border-inline-start: 3px solid transparent;
      padding-inline-start: 9px;
    }

    .provider-list-row.provider-list-row--selected {
      border-inline-start-color: var(--meter-fill);
      background: color-mix(in srgb, var(--meter-fill) 12%, var(--card));
    }

    .reorder-grip {
      position: relative;
      color: var(--tertiary);
      cursor: grab;
      font-size: 16px;
    }

    .reorder-grip::after {
      position: absolute;
      inset: -10px -8px;
      content: '';
    }

    .provider-list-main {
      display: flex;
      min-width: 0;
      flex: 1;
      align-items: center;
      flex-direction: row;
      gap: 10px;
      padding: 4px;
      border: 0;
      color: var(--text);
      background: none;
      text-align: left;
    }

    .provider-list-main > span {
      display: flex;
      min-width: 0;
      flex-direction: column;
    }

    .provider-list-main b {
      font-size: 13px;
    }

    .provider-list-main small {
      color: var(--secondary);
      font-size: 9px;
    }

    .provider-list-row {
      min-height: 42px;
      gap: 10px;
      padding: 9px 12px;
      border-top-color: var(--separator);
    }

    .provider-list-row > .provider-icon {
      color: var(--text);
    }

    .provider-list-main b {
      font-size: 14px;
      font-weight: 600;
    }

    .provider-list-main small {
      font-size: 11px;
    }

    .switch input {
      position: absolute;
    }

    .switch span {
      width: 28px;
      height: 16px;
    }

    .chevron {
      font-size: 18px;
    }
  }
</style>
