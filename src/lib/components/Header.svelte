<script lang="ts">
  import { open } from '@tauri-apps/plugin-dialog';
  import {
    appSettings,
    networkInfo,
    settingsModalOpen,
    authModalOpen,
    cloudMyDevices,
    cloudContacts,
    activeMainTab,
    cliQueuedFiles,
  } from '../stores/state';
  import { contacts, lanContacts } from '../stores/contacts';
  import { activeTransfers } from '../stores/transfers';
  import { totalUnreadCount } from '../stores/chat';
  import { raydropApi } from '../utils/tauri-ipc';
  import { updateInfo, updateModalOpen, updateChecking } from '../stores/update';
  import { checkForUpdates } from '../utils/updater';
  import {
    Copy,
    Check,
    Globe,
    Wifi,
    Settings,
    LogIn,
    LogOut,
    Mail,
    Sparkles,
    RefreshCw,
    Activity,
    Laptop,
    Users,
    Zap,
    Plus,
    MessageSquare,
  } from 'lucide-svelte';

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

<header class="border-b border-slate-800/80 bg-slate-900/80 backdrop-blur-xl px-5 py-3 flex items-center justify-between gap-4 z-20">
  <!-- Brand & Identity (Left) -->
  <div class="flex items-center gap-3 shrink-0">
    <img
      src="/icon.png"
      alt="Raydrop Icon"
      class="w-9 h-9 rounded-xl shadow-lg shadow-cyan-500/20 object-contain p-0.5 bg-slate-900 border border-cyan-500/30"
    />
    <div>
      <div class="flex items-center gap-2">
        <h1 class="text-base font-extrabold tracking-tight text-white font-display">Raydrop</h1>
        <button
          type="button"
          on:click={() => checkForUpdates(true)}
          class="text-[10px] font-mono px-1.5 py-0.5 rounded bg-slate-800 hover:bg-slate-700 text-slate-400 hover:text-white border border-slate-700 transition-colors flex items-center gap-1 cursor-pointer"
          title="Click to check for software updates"
        >
          {#if $updateChecking}
            <RefreshCw class="w-2.5 h-2.5 animate-spin text-cyan-400" />
          {/if}
          <span>v1.2.5</span>
        </button>

        {#if $updateInfo?.has_update}
          <button
            type="button"
            on:click={() => updateModalOpen.set(true)}
            class="flex items-center gap-1 text-[10px] font-mono font-bold px-2 py-0.5 rounded-full bg-cyan-500/20 hover:bg-cyan-500/30 text-cyan-300 border border-cyan-500/40 shadow-sm shadow-cyan-500/20 animate-pulse transition-all cursor-pointer"
            title="Click to view update details and install"
          >
            <Sparkles class="w-3 h-3 text-cyan-400" />
            <span>Update {$updateInfo.latest_version}</span>
          </button>
        {/if}
      </div>

      <div class="flex items-center gap-2 mt-0.5">
        {#if $appSettings.user_email}
          <Mail class="w-3 h-3 text-cyan-400 shrink-0" />
          <span class="text-xs font-mono text-cyan-300 truncate max-w-[130px]" title={$appSettings.user_email}>
            {$appSettings.user_email}
          </span>
        {:else}
          <span class="text-xs font-mono text-slate-400 truncate max-w-[130px]">{$appSettings.device_id}</span>
        {/if}
        <button
          on:click={copyIdentity}
          class="text-slate-400 hover:text-cyan-400 transition-colors p-0.5 rounded"
          title="Copy Email / Device ID"
        >
          {#if copied}
            <Check class="w-3 h-3 text-emerald-400" />
          {:else}
            <Copy class="w-3 h-3" />
          {/if}
        </button>
      </div>
    </div>
  </div>

  <!-- Center Segmented Navigation Tabs (Matching Filee / Landing Page style) -->
  <nav class="hidden md:flex items-center gap-1 bg-slate-950/80 p-1 rounded-2xl border border-slate-800/80 shadow-inner">
    <!-- 1. Transfers & Streams (Default Active Full Tab) -->
    <button
      type="button"
      on:click={() => activeMainTab.set('transfers')}
      class="flex items-center gap-2 px-4 py-1.5 rounded-xl text-xs font-bold transition-all {$activeMainTab === 'transfers'
        ? 'bg-gradient-to-r from-cyan-500/25 to-indigo-500/25 text-cyan-300 border border-cyan-500/40 shadow-sm shadow-cyan-500/15'
        : 'text-slate-400 hover:text-slate-200 hover:bg-slate-900/60'}"
    >
      <Zap class="w-3.5 h-3.5 {$activeMainTab === 'transfers' ? 'text-cyan-400' : 'text-slate-400'}" />
      <span>Transfers</span>
      {#if $activeTransfers.length > 0}
        <span class="text-[10px] px-1.5 py-0.2 rounded-full bg-cyan-400 text-slate-950 font-bold font-mono animate-pulse">
          {$activeTransfers.length}
        </span>
      {/if}
    </button>

    <!-- 2. P2P Chat Tab -->
    <button
      type="button"
      on:click={() => activeMainTab.set('chat')}
      class="flex items-center gap-1.5 px-3.5 py-1.5 rounded-xl text-xs font-semibold transition-all {$activeMainTab === 'chat'
        ? 'bg-gradient-to-r from-cyan-500/25 to-indigo-500/25 text-cyan-300 border border-cyan-500/40 shadow-sm shadow-cyan-500/15'
        : 'text-slate-400 hover:text-slate-200 hover:bg-slate-900/60'}"
    >
      <MessageSquare class="w-3.5 h-3.5 {$activeMainTab === 'chat' ? 'text-cyan-400' : 'text-slate-400'}" />
      <span>Chat</span>
      {#if $totalUnreadCount > 0}
        <span class="text-[10px] px-1.5 py-0.2 rounded-full bg-cyan-400 text-slate-950 font-bold font-mono animate-pulse">
          {$totalUnreadCount}
        </span>
      {/if}
    </button>

    <!-- 2. LAN Radar (Inactive Tab) -->
    <button
      type="button"
      on:click={() => activeMainTab.set('lan')}
      class="flex items-center gap-1.5 px-3.5 py-1.5 rounded-xl text-xs font-semibold transition-all {$activeMainTab === 'lan'
        ? 'bg-cyan-500/20 text-cyan-300 border border-cyan-500/40 shadow-sm shadow-cyan-500/15'
        : 'text-slate-400 hover:text-slate-200 hover:bg-slate-900/60'}"
    >
      <Wifi class="w-3.5 h-3.5 {$activeMainTab === 'lan' ? 'text-cyan-400' : 'text-slate-400'}" />
      <span>LAN Radar</span>
      {#if $lanContacts.length > 0}
        <span class="text-[10px] px-1.5 py-0.2 rounded-full bg-slate-800 text-cyan-300 font-mono">
          {$lanContacts.length}
        </span>
      {/if}
    </button>

    <!-- 3. My Devices (Inactive Tab) -->
    <button
      type="button"
      on:click={() => activeMainTab.set('devices')}
      class="flex items-center gap-1.5 px-3.5 py-1.5 rounded-xl text-xs font-semibold transition-all {$activeMainTab === 'devices'
        ? 'bg-cyan-500/20 text-cyan-300 border border-cyan-500/40 shadow-sm shadow-cyan-500/15'
        : 'text-slate-400 hover:text-slate-200 hover:bg-slate-900/60'}"
    >
      <Laptop class="w-3.5 h-3.5 {$activeMainTab === 'devices' ? 'text-cyan-400' : 'text-slate-400'}" />
      <span>My Devices</span>
      {#if $cloudMyDevices.length > 0}
        <span class="text-[10px] px-1.5 py-0.2 rounded-full bg-slate-800 text-slate-400 font-mono">
          {$cloudMyDevices.length}
        </span>
      {/if}
    </button>

    <!-- 4. Contacts (Inactive Tab) -->
    <button
      type="button"
      on:click={() => activeMainTab.set('contacts')}
      class="flex items-center gap-1.5 px-3.5 py-1.5 rounded-xl text-xs font-semibold transition-all {$activeMainTab === 'contacts'
        ? 'bg-cyan-500/20 text-cyan-300 border border-cyan-500/40 shadow-sm shadow-cyan-500/15'
        : 'text-slate-400 hover:text-slate-200 hover:bg-slate-900/60'}"
    >
      <Users class="w-3.5 h-3.5 {$activeMainTab === 'contacts' ? 'text-cyan-400' : 'text-slate-400'}" />
      <span>Contacts</span>
      {#if $cloudContacts.length > 0}
        <span class="text-[10px] px-1.5 py-0.2 rounded-full bg-slate-800 text-slate-400 font-mono">
          {$cloudContacts.length}
        </span>
      {/if}
    </button>
  </nav>

  <!-- Status Badges & Quick Action Controls (Right) -->
  <div class="flex items-center gap-2 shrink-0">
    <!-- WAN STUN Status -->
    <div
      class="hidden md:flex items-center gap-1.5 px-2.5 py-1 rounded-xl border text-xs {$networkInfo.signalingConnected
        ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-300'
        : 'bg-slate-800/70 border-slate-700/60 text-slate-400'}"
      title={$networkInfo.wanReflexiveAddr
        ? `Public NAT Endpoint: ${$networkInfo.wanReflexiveAddr}`
        : 'WAN Signaling & STUN Server'}
    >
      <Globe class="w-3.5 h-3.5 {$networkInfo.signalingConnected ? 'text-emerald-400' : 'text-slate-500'}" />
      <span>{$networkInfo.signalingConnected ? 'Online' : 'Standby'}</span>
    </div>

    <!-- Discoverability Toggle -->
    <button
      type="button"
      on:click={toggleDiscoverable}
      class="flex items-center gap-1.5 px-2.5 py-1 rounded-xl border text-xs font-semibold transition-all {$appSettings.discoverable
        ? 'bg-cyan-500/15 border-cyan-500/40 text-cyan-300'
        : 'bg-slate-800/60 border-slate-700 text-slate-400'}"
      title="Toggle whether nearby LAN peers can see this device"
    >
      <span
        class="w-2 h-2 rounded-full {$appSettings.discoverable
          ? 'bg-cyan-400 animate-pulse'
          : 'bg-slate-500'}"
      ></span>
      <span>{$appSettings.discoverable ? 'Visible' : 'Hidden'}</span>
    </button>

    <!-- Account Sign In / Out -->
    {#if $appSettings.auth_token}
      <button
        type="button"
        on:click={handleLogout}
        class="p-1.5 rounded-xl bg-slate-800/70 hover:bg-rose-500/20 border border-slate-700/70 hover:border-rose-500/30 text-slate-300 hover:text-rose-300 transition-colors"
        title="Sign Out ({$appSettings.user_email})"
      >
        <LogOut class="w-4 h-4" />
      </button>
    {:else}
      <button
        type="button"
        on:click={() => authModalOpen.set(true)}
        class="flex items-center gap-1.5 px-3 py-1 rounded-xl bg-gradient-to-r from-cyan-500 to-indigo-600 hover:from-cyan-400 hover:to-indigo-500 text-white font-bold text-xs transition-colors shadow-sm shadow-cyan-500/20"
      >
        <LogIn class="w-3.5 h-3.5" />
        <span>Sign In</span>
      </button>
    {/if}

    <!-- Send Files Quick Button -->
    <button
      type="button"
      on:click={async () => {
        try {
          const picked = await open({ multiple: true, title: 'Select Files to Send via Raydrop' });
          if (picked) {
            const list = Array.isArray(picked) ? picked : [picked];
            cliQueuedFiles.update((prev) => [...prev, ...list]);
          }
        } catch (e) {
          console.warn('File picker canceled or error:', e);
        }
      }}
      class="flex items-center gap-1.5 px-3 py-1.5 rounded-xl bg-gradient-to-r from-cyan-500 to-indigo-600 hover:from-cyan-400 hover:to-indigo-500 text-white font-bold text-xs shadow-md shadow-cyan-500/20 transition-all hover:-translate-y-0.5 active:translate-y-0"
      title="Select multiple files to send"
    >
      <Plus class="w-3.5 h-3.5 stroke-[2.5]" />
      <span class="hidden sm:inline">Send Files</span>
    </button>

    <!-- Settings Trigger Button -->
    <button
      type="button"
      on:click={() => settingsModalOpen.set(true)}
      class="p-1.5 rounded-xl bg-slate-800/70 hover:bg-slate-700 border border-slate-700/70 text-slate-300 hover:text-white transition-colors"
      title="Open Raydrop Settings"
    >
      <Settings class="w-4 h-4" />
    </button>
  </div>
</header>
