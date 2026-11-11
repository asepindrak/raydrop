<script lang="ts">
  import { open } from '@tauri-apps/plugin-dialog';
  import { contacts, lanContacts, wanContacts } from '../stores/contacts';
  import { raydropApi, type Contact } from '../utils/tauri-ipc';
  import {
    Wifi,
    Globe,
    UserPlus,
    Send,
    FolderUp,
    Shield,
    ShieldCheck,
    ShieldBan,
    Laptop,
  } from 'lucide-svelte';

  let activeTab: 'lan' | 'wan' = 'lan';
  let showAddWanModal = false;
  let newWanDeviceId = '';
  let newWanAlias = '';
  let newWanDirectAddr = '';
  let errorMsg = '';

  async function refreshContacts() {
    const list = await raydropApi.getContacts();
    contacts.set(list);
  }

  async function handleAddWanPeer() {
    errorMsg = '';
    if (!newWanDeviceId.trim() || !newWanAlias.trim()) {
      errorMsg = 'Device ID and Alias Name are required.';
      return;
    }
    try {
      await raydropApi.addWanContact({
        deviceId: newWanDeviceId,
        aliasName: newWanAlias,
        directIpPort: newWanDirectAddr || null,
      });
      newWanDeviceId = '';
      newWanAlias = '';
      newWanDirectAddr = '';
      showAddWanModal = false;
      await refreshContacts();
    } catch (e: any) {
      errorMsg = String(e);
    }
  }

  async function handleTrustChange(peer: Contact, nextTrust: string) {
    await raydropApi.updateContactTrust(
      peer.device_id,
      nextTrust as Contact['trust_level']
    );
    await refreshContacts();
  }

  async function pickAndSendFiles(peer: Contact, directory = false) {
    const selected = await open({
      multiple: true,
      directory,
      title: directory
        ? `Send Folder to ${peer.alias_name}`
        : `Send Files to ${peer.alias_name}`,
    });
    if (!selected) return;
    const paths = Array.isArray(selected) ? selected : [selected];
    if (paths.length > 0) {
      await raydropApi.initiateTransfer(peer.device_id, paths);
    }
  }
</script>

<div class="flex flex-col h-full bg-slate-900/40 border border-slate-800/80 rounded-2xl overflow-hidden">
  <!-- Tabs Header -->
  <div class="p-4 border-b border-slate-800/80 flex items-center justify-between">
    <div class="flex items-center gap-1.5 bg-slate-950/70 p-1 rounded-xl border border-slate-800">
      <button
        on:click={() => (activeTab = 'lan')}
        class="flex items-center gap-2 px-3.5 py-1.5 rounded-lg text-xs font-semibold transition-all {activeTab ===
        'lan'
          ? 'bg-cyan-500/20 text-cyan-300 border border-cyan-500/30'
          : 'text-slate-400 hover:text-slate-200'}"
      >
        <Wifi class="w-3.5 h-3.5" />
        <span>LAN Radar ({$lanContacts.length})</span>
      </button>
      <button
        on:click={() => (activeTab = 'wan')}
        class="flex items-center gap-2 px-3.5 py-1.5 rounded-lg text-xs font-semibold transition-all {activeTab ===
        'wan'
          ? 'bg-cyan-500/20 text-cyan-300 border border-cyan-500/30'
          : 'text-slate-400 hover:text-slate-200'}"
      >
        <Globe class="w-3.5 h-3.5" />
        <span>WAN & Saved Peers ({$wanContacts.length})</span>
      </button>
    </div>

    <button
      on:click={() => (showAddWanModal = true)}
      class="flex items-center gap-1.5 px-3 py-1.5 rounded-xl bg-cyan-500 hover:bg-cyan-400 text-slate-950 font-semibold text-xs transition-colors shadow-sm shadow-cyan-500/20"
    >
      <UserPlus class="w-3.5 h-3.5" />
      <span>Add WAN Peer</span>
    </button>
  </div>

  <!-- Contact Cards List -->
  <div class="flex-1 overflow-y-auto p-4 space-y-2.5">
    {#each activeTab === 'lan' ? $lanContacts : $wanContacts as peer (peer.device_id)}
      <div class="group p-3.5 rounded-xl bg-slate-900/90 hover:bg-slate-800/80 border border-slate-800 hover:border-cyan-500/30 transition-all flex items-center justify-between gap-3">
        <div class="flex items-center gap-3 min-w-0">
          <div class="relative w-10 h-10 rounded-xl bg-slate-800 border border-slate-700 flex items-center justify-center shrink-0">
            <Laptop class="w-5 h-5 text-cyan-400" />
            <span
              class="absolute -bottom-0.5 -right-0.5 w-2.5 h-2.5 rounded-full ring-2 ring-slate-900 {peer.is_online
                ? 'bg-emerald-400'
                : 'bg-slate-500'}"
            ></span>
          </div>
          <div class="min-w-0">
            <div class="flex items-center gap-2">
              <p class="text-sm font-semibold text-slate-100 truncate">{peer.alias_name}</p>
              <span class="text-[10px] uppercase font-mono px-1.5 py-0.5 rounded bg-slate-800 text-slate-400 border border-slate-700">
                {peer.network_type}
              </span>
            </div>
            <p class="text-xs font-mono text-slate-400 truncate">
              {peer.device_id}
              {#if peer.last_known_ip}
                <span class="text-slate-500">• {peer.last_known_ip}</span>
              {/if}
            </p>
          </div>
        </div>

        <!-- Actions & Trust Selector -->
        <div class="flex items-center gap-2 shrink-0">
          <select
            value={peer.trust_level}
            on:change={(e) => handleTrustChange(peer, e.currentTarget.value)}
            class="bg-slate-950 border border-slate-700/80 rounded-lg px-2 py-1 text-[11px] text-slate-300 focus:outline-none focus:border-cyan-500"
            title="Privacy Trust Rule"
          >
            <option value="ask">Ask Every Time</option>
            <option value="trusted_auto_accept">Auto-Accept</option>
            <option value="blocked">Blocked</option>
          </select>

          <button
            on:click={() => pickAndSendFiles(peer, false)}
            class="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-cyan-500/15 hover:bg-cyan-500 text-cyan-300 hover:text-slate-950 border border-cyan-500/30 text-xs font-semibold transition-all"
            title="Send Files"
          >
            <Send class="w-3.5 h-3.5" />
            <span>Files</span>
          </button>

          <button
            on:click={() => pickAndSendFiles(peer, true)}
            class="p-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-300 border border-slate-700 transition-colors"
            title="Send Folder"
          >
            <FolderUp class="w-4 h-4" />
          </button>
        </div>
      </div>
    {:else}
      <div class="h-56 flex flex-col items-center justify-center text-center px-6">
        <div class="w-12 h-12 rounded-2xl bg-slate-800/60 border border-slate-700/50 flex items-center justify-center mb-3 text-slate-400">
          {#if activeTab === 'lan'}
            <Wifi class="w-6 h-6 text-cyan-400/70 animate-pulse" />
          {:else}
            <Globe class="w-6 h-6 text-cyan-400/70" />
          {/if}
        </div>
        <p class="text-sm font-medium text-slate-300">
          {#if activeTab === 'lan'}
            Scanning local Wi-Fi / LAN via mDNS...
          {:else}
            No saved WAN peers yet
          {/if}
        </p>
        <p class="text-xs text-slate-500 mt-1 max-w-xs">
          {#if activeTab === 'lan'}
            Devices running Raydrop on the same network will appear here automatically.
          {:else}
            Add a remote peer's <span class="font-mono text-slate-400">RAY-XXXX-XXXX-XXXX</span> ID to transfer across the internet via P2P NAT hole-punching.
          {/if}
        </p>
      </div>
    {/each}
  </div>
</div>

<!-- Add WAN Contact Modal -->
{#if showAddWanModal}
  <div class="fixed inset-0 z-50 bg-slate-950/80 backdrop-blur-sm flex items-center justify-center p-4">
    <div class="w-full max-w-md bg-slate-900 border border-slate-800 rounded-2xl p-6 shadow-2xl">
      <h3 class="text-base font-bold text-white">Add Remote WAN / Direct Peer</h3>
      <p class="text-xs text-slate-400 mt-1">
        Enter your peer's Raydrop Device ID to connect over the internet via your Dokploy Signaling/STUN server or direct IP.
      </p>

      <div class="space-y-3 mt-4">
        <div>
          <label for="wan-device-id" class="block text-xs font-medium text-slate-300 mb-1">Peer Device ID</label>
          <input
            id="wan-device-id"
            type="text"
            bind:value={newWanDeviceId}
            placeholder="RAY-A4F8-9D12-77C1"
            class="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-sm font-mono text-white focus:outline-none focus:border-cyan-500"
          />
        </div>
        <div>
          <label for="wan-alias" class="block text-xs font-medium text-slate-300 mb-1">Alias / Nickname</label>
          <input
            id="wan-alias"
            type="text"
            bind:value={newWanAlias}
            placeholder="Office Workstation"
            class="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-sm text-white focus:outline-none focus:border-cyan-500"
          />
        </div>
        <div>
          <label for="wan-direct-addr" class="block text-xs font-medium text-slate-400 mb-1">
            Optional Direct IP:Port (Tailscale / VPN / Static IP)
          </label>
          <input
            id="wan-direct-addr"
            type="text"
            bind:value={newWanDirectAddr}
            placeholder="Leave empty for automatic WAN STUN Hole-Punching"
            class="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-xs font-mono text-slate-200 focus:outline-none focus:border-cyan-500"
          />
        </div>

        {#if errorMsg}
          <p class="text-xs text-rose-400">{errorMsg}</p>
        {/if}
      </div>

      <div class="flex items-center justify-end gap-2 mt-6">
        <button
          on:click={() => (showAddWanModal = false)}
          class="px-4 py-2 rounded-xl bg-slate-800 hover:bg-slate-700 text-xs font-semibold text-slate-300"
        >
          Cancel
        </button>
        <button
          on:click={handleAddWanPeer}
          class="px-4 py-2 rounded-xl bg-cyan-500 hover:bg-cyan-400 text-xs font-semibold text-slate-950"
        >
          Save Peer
        </button>
      </div>
    </div>
  </div>
{/if}

