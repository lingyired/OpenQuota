<script lang="ts">
  import {
    providerIconColor,
    providerIconFallbackColor,
    providerIconPath,
    providerIconSlug,
    providerIconUrl,
    providerIconViewBox,
  } from './providerIconPaths';

  interface Props {
    providerId: string;
    size?: number;
  }

  let { providerId, size = 18 }: Props = $props();
  const path = $derived(providerIconPath(providerId));
  const color = $derived(providerIconColor(providerId));
  const viewBox = $derived(providerIconViewBox(providerId));
  const url = $derived(providerIconUrl(providerId));
  const iconSlug = $derived(providerIconSlug(providerId));
  const fallbackColor = $derived(providerIconFallbackColor(providerId));
</script>

{#if url}
  <img
    class="provider-icon"
    src={url}
    data-icon={iconSlug}
    width={size}
    height={size}
    alt=""
    aria-hidden="true"
    draggable="false"
  />
{:else}
  <svg
    class="provider-icon"
    width={size}
    height={size}
    {viewBox}
    fill="none"
    aria-hidden="true"
    style={`color: ${fallbackColor}`}
  >
    <path d={path} fill={color ?? 'currentColor'} />
  </svg>
{/if}

<style>
  .provider-icon {
    display: block;
    flex: 0 0 auto;
    object-fit: contain;
  }
</style>
