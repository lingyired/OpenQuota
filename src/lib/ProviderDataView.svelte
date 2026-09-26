<script lang="ts">
  import Dashboard, { type DashboardProps } from './Dashboard.svelte';

  interface Props extends DashboardProps {
    providerId: string;
    readOnlyPreview?: boolean;
  }

  let { providerId, readOnlyPreview = false, ...dashboardProps }: Props = $props();
</script>

<div class="provider-data-view" inert={readOnlyPreview}>
  <Dashboard
    {...dashboardProps}
    focusedProviderId={providerId}
    showGlobalContent={false}
    showProviderContent={true}
  />
</div>

<style>
  .provider-data-view {
    display: contents;
  }
</style>
