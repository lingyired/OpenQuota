<script lang="ts">
  import openQuotaMark from '../assets/usage01-mark.png';

  interface Props {
    size?: number;
    decorative?: boolean;
  }

  let { size = 18, decorative = true }: Props = $props();
</script>

<img
  src={openQuotaMark}
  width={size}
  height={size}
  alt={decorative ? '' : 'Usage01'}
  aria-hidden={decorative}
  draggable="false"
/>

<style>
  img {
    display: block;
    object-fit: contain;
  }
</style>
