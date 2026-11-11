<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { appSettings, settingsModalOpen } from "../stores/state";
  import { raydropApi } from "../utils/tauri-ipc";
  import { FolderOpen, Terminal, Save, X, CheckCircle2 } from "lucide-svelte";

  let aliasName = "";
  let downloadDir = "";
  let signalingUrl = "";
  let stunAddr = "";
  let statusBanner = "";

  $: if ($settingsModalOpen) {
    aliasName = $appSettings.alias_name;
    downloadDir = $appSettings.download_dir;
    signalingUrl = $appSettings.signaling_url;
    stunAddr = $appSettings.stun_addr;
  }

  async function browseDownloadDir() {
    const dir = await open({
      directory: true,
      multiple: false,
      defaultPath: downloadDir,
      title: "Select Default Download Directory",
    });
    if (typeof dir === "string") {
      downloadDir = dir;
    }
  }

  async function saveSettings() {
    const updated = await raydropApi.updateSettings({
      aliasName,
      downloadDir,
      signalingUrl,
      stunAddr,
    });
    appSettings.set(updated);
    statusBanner =
      "Settings saved! Signaling client will reconnect automatically.";
    setTimeout(() => (statusBanner = ""), 3000);
  }

  async function handleRegisterContextMenu() {
    try {
      const msg = await raydropApi.registerOsContextMenu();
      statusBanner = msg;
    } catch (e: any) {
      statusBanner = `Context menu error: ${String(e)}`;
    }
  }
</script>

{#if $settingsModalOpen}
  <div
    class="fixed inset-0 z-50 bg-slate-950/80 backdrop-blur-sm flex items-center justify-center p-4"
  >
    <div
      class="w-full max-w-lg bg-slate-900 border border-slate-800 rounded-2xl p-6 shadow-2xl"
    >
      <div
        class="flex items-center justify-between pb-3 border-b border-slate-800"
      >
        <div>
          <h3 class="text-base font-bold text-white">Raydrop Configuration</h3>
          <p class="text-xs text-slate-400">
            Configure Device Alias, Dokploy WAN Signaling/STUN endpoints, and OS
            Context Menus
          </p>
        </div>
        <button
          on:click={() => settingsModalOpen.set(false)}
          class="p-1.5 rounded-lg text-slate-400 hover:text-white hover:bg-slate-800"
        >
          <X class="w-4 h-4" />
        </button>
      </div>

      <div class="space-y-4 mt-4">
        <div>
          <label
            for="setting-alias"
            class="block text-xs font-medium text-slate-300 mb-1"
            >Device Alias Name</label
          >
          <input
            id="setting-alias"
            type="text"
            bind:value={aliasName}
            class="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-xs text-white focus:outline-none focus:border-cyan-500"
          />
        </div>

        <div>
          <label
            for="setting-dl-dir"
            class="block text-xs font-medium text-slate-300 mb-1"
          >
            Default Download Folder (Used for Auto-Accept & Default Prompt)
          </label>
          <div class="flex items-center gap-2">
            <input
              id="setting-dl-dir"
              type="text"
              bind:value={downloadDir}
              class="flex-1 px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-xs font-mono text-slate-200 focus:outline-none focus:border-cyan-500"
            />
            <button
              on:click={browseDownloadDir}
              class="px-3 py-2 rounded-xl bg-slate-800 hover:bg-slate-700 border border-slate-700 text-xs text-slate-200 flex items-center gap-1.5"
            >
              <FolderOpen class="w-3.5 h-3.5 text-cyan-400" />
              <span>Browse</span>
            </button>
          </div>
        </div>

        <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
          <div>
            <label
              for="setting-sig-url"
              class="block text-xs font-medium text-slate-300 mb-1"
            >
              WAN Signaling WSS URL (Traefik)
            </label>
            <input
              id="setting-sig-url"
              type="text"
              bind:value={signalingUrl}
              placeholder="wss://raydrop.gapleh.online/ws"
              class="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-xs font-mono text-cyan-300 focus:outline-none focus:border-cyan-500"
            />
          </div>
          <div>
            <label
              for="setting-stun-addr"
              class="block text-xs font-medium text-slate-300 mb-1"
            >
              UDP STUN Server (Port 3478)
            </label>
            <input
              id="setting-stun-addr"
              type="text"
              bind:value={stunAddr}
              placeholder="raydrop.gapleh.online:3478"
              class="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-xs font-mono text-cyan-300 focus:outline-none focus:border-cyan-500"
            />
          </div>
        </div>

        <!-- OS Context Menu Integration -->
        <div
          class="p-3.5 rounded-xl bg-slate-950/90 border border-slate-800 flex items-center justify-between gap-3"
        >
          <div>
            <p class="text-xs font-semibold text-slate-200">
              Native OS Right-Click Integration
            </p>
            <p class="text-[11px] text-slate-400">
              Adds "Send with Raydrop" to Windows Explorer (Files, Folders &
              SendTo), Ubuntu Nautilus, or macOS Finder.
            </p>
          </div>
          <button
            on:click={handleRegisterContextMenu}
            class="px-3 py-2 rounded-xl bg-slate-800 hover:bg-slate-700 border border-cyan-500/30 text-cyan-300 text-xs font-semibold flex items-center gap-1.5 shrink-0"
          >
            <Terminal class="w-3.5 h-3.5" />
            <span>Register Menu</span>
          </button>
        </div>

        {#if statusBanner}
          <div
            class="flex items-center gap-2 text-xs text-emerald-300 bg-emerald-500/10 border border-emerald-500/30 px-3 py-2 rounded-xl"
          >
            <CheckCircle2 class="w-4 h-4 shrink-0" />
            <span>{statusBanner}</span>
          </div>
        {/if}
      </div>

      <div class="flex items-center justify-end gap-2 mt-6">
        <button
          on:click={() => settingsModalOpen.set(false)}
          class="px-4 py-2 rounded-xl bg-slate-800 hover:bg-slate-700 text-xs font-semibold text-slate-300"
        >
          Close
        </button>
        <button
          on:click={saveSettings}
          class="flex items-center gap-1.5 px-4 py-2 rounded-xl bg-cyan-500 hover:bg-cyan-400 text-xs font-bold text-slate-950"
        >
          <Save class="w-3.5 h-3.5" />
          <span>Save Configuration</span>
        </button>
      </div>
    </div>
  </div>
{/if}
