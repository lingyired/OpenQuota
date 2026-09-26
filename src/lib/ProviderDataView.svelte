<script lang="ts">
  import Dashboard, { type DashboardProps } from './Dashboard.svelte';
  import { tStore } from './i18n';

  interface Props extends DashboardProps {
    providerId: string;
    readOnlyPreview?: boolean;
  }

  let { providerId, readOnlyPreview = false, ...dashboardProps }: Props = $props();
  let lastAutoRefreshProviderId: string | null = null;

  $effect(() => {
    if (providerId === lastAutoRefreshProviderId) return;
    lastAutoRefreshProviderId = providerId;
    void dashboardProps.onRefreshIfDue?.(providerId);
  });
</script>

<div class="provider-data-view" class:provider-data-view--preview={readOnlyPreview}>
  {#if readOnlyPreview}
    <button
      class="provider-data-view__refresh"
      type="button"
      aria-label={$tStore('dashboard.refreshProvider', {
        provider: dashboardProps.catalog.displayName(
          providerId,
          dashboardProps.settings.providerNames,
        ),
      })}
      onclick={() => dashboardProps.onRefresh(providerId)}
    >
      {$tStore('dashboard.refreshProvider', {
        provider: dashboardProps.catalog.displayName(
          providerId,
          dashboardProps.settings.providerNames,
        ),
      })}
    </button>
  {/if}
  <div class="provider-data-view__dashboard">
    <Dashboard
      {...dashboardProps}
      focusedProviderId={providerId}
      showGlobalContent={false}
      showProviderContent={true}
      {readOnlyPreview}
    />
  </div>
</div>

<style>
  .provider-data-view {
    display: contents;
  }

  .provider-data-view--preview {
    display: flex;
    min-width: 0;
    min-height: 0;
    flex-direction: column;
  }

  .provider-data-view__refresh {
    flex: 0 0 auto;
    align-self: flex-end;
    margin: 8px 10px 0;
    padding: 5px 9px;
    border: 1px solid var(--separator);
    border-radius: 7px;
    color: var(--secondary);
    background: var(--tray);
    font: inherit;
    font-size: 11px;
    cursor: pointer;
  }

  .provider-data-view__dashboard {
    min-width: 0;
    min-height: 0;
  }

  .provider-data-view--preview .provider-data-view__dashboard {
    flex: 1 1 auto;
    overflow: auto;
  }
</style>
