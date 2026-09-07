<script lang="ts">
  import { tBackendStore, tStore } from '../lib/i18n';
</script>

<p data-testid="static">{$tStore('settings.language')}</p>
<p data-testid="backend">{$tBackendStore('Usage Today')}</p>
