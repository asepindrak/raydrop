<script lang="ts">
  import { appSettings, networkInfo, settingsModalOpen } from '../stores/state';
  import { raydropApi } from '../utils/tauri-ipc';
  import { Radio, Copy, Check, Globe, Wifi, Settings, ShieldCheck } from 'lucide-svelte';

  let copied = false;

  async function toggleDiscoverable() {
    const next = !$appSettings.discoverable;
    const res = await raydropApi.toggleDiscoverability(next);
    appSettings.update((s) => ({ ...s, discoverable: res }));
  }

  async function copyDeviceId() {
    await navigator.clipboard.writeText($appSettings.device_id);
    copied = true;
    setTimeout(() => (copied = false), 1800);
  }
</script>

<header class="border-b border-slate-800/80 bg-slate-900/70 backdrop-blur-md px-6 py-3.5 flex items-center justify-between">
  <!-- Brand & Identity -->
  <div class="flex items-center gap-4">
    <div class="w-10 h-10 rounded-xl bg-gradient-to-br from-cyan-400 to-blue-600 flex items-center justify-center shadow-lg shadow-cyan-500/20">
      <Radio class="w-5 h-5 text-slate-950 stroke-[2.5]" />
    </div>
    <div>
      <div class="flex items-center gap-2.5">
        <h1 class="text-lg font-bold tracking-tight text-white">Raydrop</h1>
        <span class="text-xs font-medium px-2 py-0.5 rounded-full bg-cyan-500/10 text-cyan-300 border border-cyan-500/20">
          {$appSettings.alias_name}
        </span>
      </div>
      <div class="flex items-center gap-2 mt-0.5">
        <span class="text-xs font-mono text-slate-400">{$appSettings.device_id}</span>
        <button
          on:click={copyDeviceId}
          class="text-slate-400 hover:text-cyan-400 transition-colors p-0.5 rounded"
          title="Copy Device ID for WAN sharing"
        >
          {#if copied}
            <Check class="w-3.5 h-3.5 text-emerald-400" />
          {:else}
            <Copy class="w-3.5 h-3.5" />
          {/if}
        </button>
      </div>
    </div>
  </div>

  <!-- Network Badges & Discoverability Toggle -->
  <div class="flex items-center gap-3">
    <!-- LAN IP Badge -->
    <div class="hidden md:flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-slate-800/70 border border-slate-700/60 text-xs text-slate-300">
      <Wifi class="w-3.5 h-3.5 text-cyan-400" />
      <span>LAN:</span>
      <span class="font-mono text-slate-200">{$networkInfo.localIp}:{$networkInfo.listenPort}</span>
    </div>

    <!-- WAN Signaling Badge -->
    <div
      class="flex items-center gap-1.5 px-3 py-1.5 rounded-lg border text-xs {$networkInfo.signalingConnected
        ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-300'
        : 'bg-slate-800/70 border-slate-700/60 text-slate-400'}"
      title={$networkInfo.wanReflexiveAddr
        ? `Public NAT Endpoint: ${$networkInfo.wanReflexiveAddr}`
        : 'Configure Dokploy Signaling Server in Settings for WAN P2P'}
    >
      <Globe class="w-3.5 h-3.5 {$networkInfo.signalingConnected ? 'text-emerald-400' : 'text-slate-500'}" />
      <span>WAN: {$networkInfo.signalingConnected ? 'Online' : 'Standby'}</span>
    </div>

    <!-- Discoverable Toggle Switch -->
    <button
      on:click={toggleDiscoverable}
      class="flex items-center gap-2.5 px-3.5 py-1.5 rounded-xl border text-xs font-semibold transition-all {$appSettings.discoverable
        ? 'bg-cyan-500/15 border-cyan-500/40 text-cyan-300 shadow-sm shadow-cyan-500/10'
        : 'bg-slate-800/60 border-slate-700 text-slate-400'}"
    >
      <span
        class="w-2 h-2 rounded-full {$appSettings.discoverable
          ? 'bg-cyan-400 animate-pulse'
          : 'bg-slate-500'}"
      ></span>
      <span>{$appSettings.discoverable ? 'Discoverable: ON' : 'Hidden: OFF'}</span>
    </button>

    <!-- Settings Button -->
    <button
      on:click={() => settingsModalOpen.set(true)}
      class="p-2 rounded-xl bg-slate-800/70 hover:bg-slate-800 border border-slate-700/70 text-slate-300 hover:text-cyan-300 transition-colors"
      title="Settings & OS Context Menu"
    >
      <Settings class="w-4 h-4" />
    </button>
  </div>
</header>

