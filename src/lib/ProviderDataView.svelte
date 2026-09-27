<script lang="ts">
  import Dashboard, { type DashboardProps } from './Dashboard.svelte';

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
    display: block;
    width: 100%;
    min-width: 0;
  }

  .provider-data-view__dashboard {
    min-width: 0;
    min-height: 0;
  }

  .provider-data-view--preview .provider-data-view__dashboard {
    width: 100%;
    overflow: visible;
  }
</style>
