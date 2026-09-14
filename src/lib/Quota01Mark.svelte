<script lang="ts">
  import quota01Mark from '../assets/quota01-mark.png';

  interface Props {
    size?: number;
    decorative?: boolean;
  }

  let { size = 18, decorative = true }: Props = $props();
</script>

<img
  src={quota01Mark}
  width={size}
  height={size}
  alt={decorative ? '' : 'Quota01'}
  aria-hidden={decorative}
  draggable="false"
/>

<style>
  img {
    display: block;
    object-fit: contain;
  }
</style>
