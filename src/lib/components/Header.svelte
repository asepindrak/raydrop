<script lang="ts">
  import {
    appSettings,
    networkInfo,
    settingsModalOpen,
    authModalOpen,
    cloudMyDevices,
    cloudContacts,
  } from '../stores/state';
  import { raydropApi } from '../utils/tauri-ipc';
  import { Copy, Check, Globe, Wifi, Settings, LogIn, LogOut, Mail } from 'lucide-svelte';

  let copied = false;

  async function toggleDiscoverable() {
    const next = !$appSettings.discoverable;
    const res = await raydropApi.toggleDiscoverability(next);
    appSettings.update((s) => ({ ...s, discoverable: res }));
  }

  async function copyIdentity() {
    const text = $appSettings.user_email || $appSettings.device_id;
    await navigator.clipboard.writeText(text);
    copied = true;
    setTimeout(() => (copied = false), 1800);
  }

  async function handleLogout() {
    const updated = await raydropApi.logoutCloudSession();
    appSettings.set(updated);
    cloudMyDevices.set([]);
    cloudContacts.set([]);
  }
</script>

<header class="border-b border-slate-800/80 bg-slate-900/70 backdrop-blur-md px-6 py-3.5 flex items-center justify-between">
  <!-- Brand & User Identity -->
  <div class="flex items-center gap-3">
    <img
      src="/icon.png"
      alt="Raydrop Icon"
      class="w-10 h-10 rounded-xl shadow-lg shadow-cyan-500/20 object-contain p-0.5 bg-slate-900/80 border border-cyan-500/30"
    />
    <div>
      <div class="flex items-center gap-2.5">
        <h1 class="text-lg font-bold tracking-tight text-white">Raydrop</h1>
        <span class="text-[10px] font-mono px-1.5 py-0.5 rounded bg-slate-800 text-slate-400 border border-slate-700">
          v1.0.6
        </span>
        {#if $appSettings.user_name}
          <span class="text-xs font-semibold px-2.5 py-0.5 rounded-full bg-emerald-500/15 text-emerald-300 border border-emerald-500/30">
            {$appSettings.user_name}
          </span>
        {/if}
        <span class="text-xs font-medium px-2 py-0.5 rounded-full bg-cyan-500/10 text-cyan-300 border border-cyan-500/20">
          {$appSettings.alias_name}
        </span>
      </div>

      <div class="flex items-center gap-2 mt-0.5">
        {#if $appSettings.user_email}
          <Mail class="w-3 h-3 text-cyan-400" />
          <span class="text-xs font-mono text-cyan-300">{$appSettings.user_email}</span>
          <span class="text-[11px] font-mono text-slate-500">({$appSettings.device_id})</span>
        {:else}
          <span class="text-xs font-mono text-slate-400">{$appSettings.device_id}</span>
        {/if}
        <button
          on:click={copyIdentity}
          class="text-slate-400 hover:text-cyan-400 transition-colors p-0.5 rounded"
          title="Copy Email / Device ID"
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

  <!-- Network Badges, Account & Discoverability Toggle -->
  <div class="flex items-center gap-2.5">
    <!-- LAN IP Badge -->
    <div class="hidden xl:flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-slate-800/70 border border-slate-700/60 text-xs text-slate-300">
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
        : 'WAN Signaling Status'}
    >
      <Globe class="w-3.5 h-3.5 {$networkInfo.signalingConnected ? 'text-emerald-400' : 'text-slate-500'}" />
      <span>WAN: {$networkInfo.signalingConnected ? 'Online' : 'Standby'}</span>
    </div>

    <!-- Discoverable Toggle Switch -->
    <button
      on:click={toggleDiscoverable}
      class="flex items-center gap-2 px-3 py-1.5 rounded-xl border text-xs font-semibold transition-all {$appSettings.discoverable
        ? 'bg-cyan-500/15 border-cyan-500/40 text-cyan-300 shadow-sm shadow-cyan-500/10'
        : 'bg-slate-800/60 border-slate-700 text-slate-400'}"
    >
      <span
        class="w-2 h-2 rounded-full {$appSettings.discoverable
          ? 'bg-cyan-400 animate-pulse'
          : 'bg-slate-500'}"
      ></span>
      <span>{$appSettings.discoverable ? 'Discoverable' : 'Hidden'}</span>
    </button>

    <!-- Account Sign In / Out -->
    {#if $appSettings.auth_token}
      <button
        on:click={handleLogout}
        class="p-2 rounded-xl bg-slate-800/70 hover:bg-rose-500/20 border border-slate-700/70 hover:border-rose-500/30 text-slate-300 hover:text-rose-300 transition-colors"
        title="Sign Out ({$appSettings.user_email})"
      >
        <LogOut class="w-4 h-4" />
      </button>
    {:else}
      <button
        on:click={() => authModalOpen.set(true)}
        class="flex items-center gap-1.5 px-3 py-1.5 rounded-xl bg-cyan-500 hover:bg-cyan-400 text-slate-950 font-bold text-xs transition-colors"
      >
        <LogIn class="w-3.5 h-3.5" />
        <span>Sign In</span>
      </button>
    {/if}

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
