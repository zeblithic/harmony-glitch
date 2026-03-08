<script lang="ts">
  import { onDestroy } from 'svelte';
  import { getNetworkStatus } from '../ipc';

  let peerCount = $state(0);

  const interval = setInterval(async () => {
    try {
      const status = await getNetworkStatus();
      peerCount = status.peerCount;
    } catch {
      /* ignore polling errors */
    }
  }, 2000);

  onDestroy(() => clearInterval(interval));
</script>

<div class="network-status" aria-live="polite">
  {peerCount} peer{peerCount === 1 ? '' : 's'} connected
</div>

<style>
  .network-status {
    position: fixed;
    bottom: 8px;
    right: 8px;
    padding: 4px 8px;
    font-size: 12px;
    color: rgba(255, 255, 255, 0.7);
    background: rgba(0, 0, 0, 0.3);
    border-radius: 4px;
    z-index: 50;
    pointer-events: none;
  }
</style>
