<script lang="ts">
  import { tick } from 'svelte';
  import { open } from '@tauri-apps/plugin-dialog';
  import { appSettings, incomingRequestQueue, cliQueuedFiles } from '../stores/state';
  import { contacts } from '../stores/contacts';
  import { raydropApi, formatBytes, type Contact } from '../utils/tauri-ipc';
  import {
    Download,
    FolderOpen,
    Check,
    X,
    Send,
    FileText,
    Laptop,
    Users,
    Wifi,
    ArrowRight,
  } from 'lucide-svelte';

  let customTargetDir = '';
  let selectedPeerForCli = '';
  let peerSelectEl: HTMLSelectElement | null = null;

  $: currentIncoming = $incomingRequestQueue[0] ?? null;
  $: if (currentIncoming && !customTargetDir) {
    customTargetDir = $appSettings.download_dir;
  }

  function getBaseName(path: string): string {
    const parts = path.split(/[/\\]/);
    return parts[parts.length - 1] || path;
  }

  // Auto-select preferred peer and auto-focus when CLI send modal opens
  $: if ($cliQueuedFiles.length > 0) {
    if (!selectedPeerForCli && $contacts.length > 0) {
      const preferred = $contacts.find((c) => c.is_online) || $contacts[0];
      if (preferred) {
        selectedPeerForCli = preferred.device_id;
      }
    }
    tick().then(() => {
      setTimeout(() => {
        peerSelectEl?.focus();
      }, 50);
    });
  }

  function handleCliKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      cliQueuedFiles.set([]);
    } else if (e.key === 'Enter' && selectedPeerForCli) {
      confirmCliSend();
    }
  }

  async function chooseTargetFolder() {
    const picked = await open({
      directory: true,
      multiple: false,
      defaultPath: customTargetDir || $appSettings.download_dir,
      title: 'Select Destination Folder',
    });
    if (typeof picked === 'string') {
      customTargetDir = picked;
    }
  }

  async function respondIncoming(accepted: boolean) {
    if (!currentIncoming) return;
    const sid = currentIncoming.session_id;
    await raydropApi.respondTransferRequest(
      sid,
      accepted,
      accepted ? customTargetDir || $appSettings.download_dir : null
    );
    incomingRequestQueue.update((q) => q.slice(1));
    customTargetDir = '';
  }

  async function confirmCliSend(targetDeviceId?: string) {
    const target = targetDeviceId || selectedPeerForCli;
    if (!target || $cliQueuedFiles.length === 0) return;
    const files = [...$cliQueuedFiles];
    cliQueuedFiles.set([]);
    await raydropApi.initiateTransfer(target, files);
  }
</script>

<!-- Incoming Transfer Request Modal -->
{#if currentIncoming}
  <div class="fixed inset-0 z-50 bg-slate-950/80 backdrop-blur-sm flex items-center justify-center p-4">
    <div class="w-full max-w-lg bg-slate-900 border border-cyan-500/30 rounded-2xl p-6 shadow-2xl">
      <div class="flex items-center gap-3">
        <div class="w-11 h-11 rounded-xl bg-cyan-500/15 border border-cyan-500/30 flex items-center justify-center text-cyan-400">
          <Download class="w-6 h-6" />
        </div>
        <div>
          <div class="flex items-center gap-2">
            <h3 class="text-base font-bold text-white">Incoming File Transfer</h3>
            <span class="text-[10px] font-mono uppercase px-2 py-0.5 rounded bg-cyan-500/15 text-cyan-300 border border-cyan-500/30">
              {currentIncoming.network_route === 'lan_direct' ? 'LAN Direct' : 'WAN P2P'}
            </span>
          </div>
          <p class="text-xs text-slate-400">
            From <span class="text-slate-200 font-semibold">{currentIncoming.sender_alias}</span>
            <span class="font-mono text-slate-500">({currentIncoming.sender_device_id})</span>
          </p>
        </div>
      </div>

      <!-- File List Summary -->
      <div class="mt-4 bg-slate-950/80 border border-slate-800 rounded-xl p-3 max-h-44 overflow-y-auto space-y-1.5">
        {#each currentIncoming.files as f}
          <div class="flex items-center justify-between text-xs py-1 border-b border-slate-800/60 last:border-none">
            <div class="flex items-center gap-2 truncate pr-2">
              <FileText class="w-3.5 h-3.5 text-cyan-400 shrink-0" />
              <span class="text-slate-200 truncate">{f.relative_path}</span>
            </div>
            <span class="font-mono text-slate-400 shrink-0">{formatBytes(f.size_bytes)}</span>
          </div>
        {/each}
      </div>

      <div class="mt-3 flex items-center justify-between text-xs text-slate-400">
        <span>Total Files: <strong class="text-slate-200">{currentIncoming.total_files}</strong></span>
        <span>Total Size: <strong class="text-cyan-300 font-mono">{formatBytes(currentIncoming.total_size_bytes)}</strong></span>
      </div>

      <!-- Save Location Picker -->
      <div class="mt-4">
        <label for="save-target-dir" class="block text-xs font-medium text-slate-300 mb-1">Save to Directory</label>
        <div class="flex items-center gap-2">
          <input
            id="save-target-dir"
            type="text"
            bind:value={customTargetDir}
            class="flex-1 px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-xs font-mono text-slate-200 focus:outline-none focus:border-cyan-500"
          />
          <button
            on:click={chooseTargetFolder}
            class="px-3 py-2 rounded-xl bg-slate-800 hover:bg-slate-700 border border-slate-700 text-xs text-slate-200 flex items-center gap-1.5"
          >
            <FolderOpen class="w-3.5 h-3.5 text-cyan-400" />
            <span>Browse</span>
          </button>
        </div>
      </div>

      <div class="flex items-center justify-end gap-2.5 mt-6">
        <button
          on:click={() => respondIncoming(false)}
          class="flex items-center gap-1.5 px-4 py-2 rounded-xl bg-rose-500/15 hover:bg-rose-500/25 border border-rose-500/30 text-rose-300 text-xs font-semibold"
        >
          <X class="w-4 h-4" />
          <span>Reject</span>
        </button>
        <button
          on:click={() => respondIncoming(true)}
          class="flex items-center gap-1.5 px-5 py-2 rounded-xl bg-cyan-500 hover:bg-cyan-400 text-slate-950 text-xs font-bold shadow-lg shadow-cyan-500/20"
        >
          <Check class="w-4 h-4" />
          <span>Accept & Stream</span>
        </button>
      </div>
    </div>
  </div>
{/if}

<!-- Outgoing OS Context Menu (--send) Peer Picker Modal -->
{#if $cliQueuedFiles.length > 0}
  <div
    class="fixed inset-0 z-50 bg-slate-950/80 backdrop-blur-sm flex items-center justify-center p-4"
    on:keydown={handleCliKeydown}
    role="presentation"
  >
    <div class="w-full max-w-lg bg-slate-900 border border-cyan-500/30 rounded-2xl p-6 shadow-2xl space-y-4">
      <div class="flex items-center justify-between">
        <div class="flex items-center gap-3">
          <div class="w-10 h-10 rounded-xl bg-cyan-500/15 border border-cyan-500/30 flex items-center justify-center text-cyan-400">
            <Send class="w-5 h-5" />
          </div>
          <div>
            <h3 class="text-base font-bold text-white">Send Selected File(s)</h3>
            <p class="text-xs text-slate-400">
              {$cliQueuedFiles.length} item(s) selected from File Explorer
            </p>
          </div>
        </div>
        <button
          on:click={() => cliQueuedFiles.set([])}
          class="p-1 rounded-lg text-slate-400 hover:text-white hover:bg-slate-800"
          title="Close"
        >
          <X class="w-4 h-4" />
        </button>
      </div>

      <!-- File List -->
      <div class="bg-slate-950 border border-slate-800 rounded-xl p-2.5 max-h-28 overflow-y-auto space-y-1">
        {#each $cliQueuedFiles as path}
          <div class="flex items-center gap-1.5 text-xs text-slate-200">
            <FileText class="w-3.5 h-3.5 text-cyan-400 shrink-0" />
            <span class="font-medium truncate">{getBaseName(path)}</span>
            <span class="text-[10px] font-mono text-slate-500 truncate">({path})</span>
          </div>
        {/each}
      </div>

      <!-- Quick 1-Click Contacts Selector -->
      <div>
        <label for="cli-recipient-peer" class="block text-xs font-semibold text-slate-200 mb-1.5">
          Select Recipient Device / Contact (Click directly or select from dropdown)
        </label>

        {#if $contacts.length > 0}
          <div class="grid grid-cols-1 sm:grid-cols-2 gap-2 mb-3 max-h-40 overflow-y-auto pr-0.5">
            {#each $contacts as peer}
              {@const isSelected = selectedPeerForCli === peer.device_id}
              <button
                on:click={() => confirmCliSend(peer.device_id)}
                class="p-2.5 rounded-xl border text-left flex items-center justify-between gap-2 transition-all {isSelected
                  ? 'bg-cyan-500/15 border-cyan-500/50 ring-1 ring-cyan-500/30'
                  : 'bg-slate-950/80 border-slate-800 hover:border-slate-700'}"
              >
                <div class="flex items-center gap-2 min-w-0">
                  <div class="relative w-7 h-7 rounded-lg bg-slate-800 flex items-center justify-center shrink-0">
                    {#if peer.is_own_device}
                      <Laptop class="w-3.5 h-3.5 text-cyan-400" />
                    {:else if peer.network_type === 'lan'}
                      <Wifi class="w-3.5 h-3.5 text-emerald-400" />
                    {:else}
                      <Users class="w-3.5 h-3.5 text-blue-400" />
                    {/if}
                    <span
                      class="absolute -bottom-0.5 -right-0.5 w-2 h-2 rounded-full {peer.is_online ? 'bg-emerald-400' : 'bg-slate-500'}"
                    ></span>
                  </div>
                  <div class="min-w-0">
                    <p class="text-xs font-semibold text-slate-100 truncate">{peer.alias_name}</p>
                    <p class="text-[10px] font-mono text-slate-400 truncate">
                      {peer.user_email || (peer.network_type === 'lan' ? 'LAN Direct' : peer.device_id)}
                    </p>
                  </div>
                </div>
                <ArrowRight class="w-3.5 h-3.5 text-slate-400 shrink-0" />
              </button>
            {/each}
          </div>
        {/if}

        <!-- Dropdown with Auto-Focus for fast keyboard entry -->
        <select
          id="cli-recipient-peer"
          bind:this={peerSelectEl}
          bind:value={selectedPeerForCli}
          autofocus
          class="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-xs text-white focus:outline-none focus:border-cyan-500 ring-1 focus:ring-cyan-500/30"
        >
          <option value="">-- Or Select from Contact / Device List --</option>
          {#each $contacts as peer}
            <option value={peer.device_id}>
              {peer.is_own_device ? '💻 [My Device] ' : ''}{peer.alias_name} {peer.user_email ? `(${peer.user_email})` : `(${peer.network_type.toUpperCase()})`} {peer.is_online ? '• Online' : ''}
            </option>
          {/each}
        </select>
      </div>

      <div class="flex items-center justify-between pt-2 border-t border-slate-800/80">
        <span class="text-[11px] text-slate-400 font-mono">Press Enter to send immediately</span>
        <div class="flex items-center gap-2">
          <button
            on:click={() => cliQueuedFiles.set([])}
            class="px-4 py-2 rounded-xl bg-slate-800 hover:bg-slate-700 text-xs font-semibold text-slate-300 transition"
          >
            Cancel
          </button>
          <button
            on:click={() => confirmCliSend()}
            disabled={!selectedPeerForCli}
            class="px-5 py-2 rounded-xl bg-cyan-500 hover:bg-cyan-400 disabled:opacity-40 text-xs font-bold text-slate-950 transition shadow-lg shadow-cyan-500/20"
          >
            Send Files Now (Enter)
          </button>
        </div>
      </div>
    </div>
  </div>
{/if}
