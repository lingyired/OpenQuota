<script lang="ts">
  import { tStore } from './i18n';
  import ProviderSessionActions from './ProviderSessionActions.svelte';

  interface Props {
    providerId: string;
    providerName: string;
  }

  let { providerId, providerName }: Props = $props();
</script>

<section class="session-section" aria-label={`${providerName} ${$tStore('providerSession.title')}`}>
  <h2>{$tStore('providerSession.title')}</h2>
  <div class="session-card">
    <ProviderSessionActions {providerId} {providerName} />
  </div>
</section>

<style>
  .session-section {
    margin-bottom: 14px;
  }

  .session-section h2 {
    margin: 0 0 8px;
    color: var(--secondary);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  .session-card {
    overflow: hidden;
    border: 1px solid var(--separator);
    border-radius: 12px;
    background: var(--surface);
  }
</style>
