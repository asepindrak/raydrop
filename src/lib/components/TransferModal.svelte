<script lang="ts">
  import { open } from '@tauri-apps/plugin-dialog';
  import { appSettings, incomingRequestQueue, cliQueuedFiles } from '../stores/state';
  import { contacts } from '../stores/contacts';
  import { raydropApi, formatBytes } from '../utils/tauri-ipc';
  import { Download, FolderOpen, Check, X, Send, FileText } from 'lucide-svelte';

  let customTargetDir = '';
  let selectedPeerForCli = '';

  $: currentIncoming = $incomingRequestQueue[0] ?? null;
  $: if (currentIncoming && !customTargetDir) {
    customTargetDir = $appSettings.download_dir;
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

  async function confirmCliSend() {
    if (!selectedPeerForCli || $cliQueuedFiles.length === 0) return;
    const files = [...$cliQueuedFiles];
    cliQueuedFiles.set([]);
    await raydropApi.initiateTransfer(selectedPeerForCli, files);
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
  <div class="fixed inset-0 z-50 bg-slate-950/80 backdrop-blur-sm flex items-center justify-center p-4">
    <div class="w-full max-w-md bg-slate-900 border border-cyan-500/30 rounded-2xl p-6 shadow-2xl">
      <div class="flex items-center gap-3">
        <div class="w-10 h-10 rounded-xl bg-cyan-500/15 border border-cyan-500/30 flex items-center justify-center text-cyan-400">
          <Send class="w-5 h-5" />
        </div>
        <div>
          <h3 class="text-base font-bold text-white">Send Selected Items</h3>
          <p class="text-xs text-slate-400">
            {$cliQueuedFiles.length} item(s) queued from OS right-click menu
          </p>
        </div>
      </div>

      <div class="mt-3 bg-slate-950 border border-slate-800 rounded-xl p-2.5 max-h-28 overflow-y-auto space-y-1">
        {#each $cliQueuedFiles as path}
          <p class="text-[11px] font-mono text-slate-300 truncate">{path}</p>
        {/each}
      </div>

      <div class="mt-4">
        <label for="cli-recipient-peer" class="block text-xs font-medium text-slate-300 mb-1.5">Select Recipient Peer</label>
        <select
          id="cli-recipient-peer"
          bind:value={selectedPeerForCli}
          class="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-xs text-white focus:outline-none focus:border-cyan-500"
        >
          <option value="">-- Choose a LAN or WAN Peer --</option>
          {#each $contacts as peer}
            <option value={peer.device_id}>
              {peer.alias_name} ({peer.network_type.toUpperCase()} • {peer.device_id})
            </option>
          {/each}
        </select>
      </div>

      <div class="flex items-center justify-end gap-2 mt-6">
        <button
          on:click={() => cliQueuedFiles.set([])}
          class="px-4 py-2 rounded-xl bg-slate-800 hover:bg-slate-700 text-xs font-semibold text-slate-300"
        >
          Cancel
        </button>
        <button
          on:click={confirmCliSend}
          disabled={!selectedPeerForCli}
          class="px-4 py-2 rounded-xl bg-cyan-500 hover:bg-cyan-400 disabled:opacity-40 text-xs font-bold text-slate-950"
        >
          Start P2P Transfer
        </button>
      </div>
    </div>
  </div>
{/if}

