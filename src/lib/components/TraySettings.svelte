<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { appSettings, settingsModalOpen } from "../stores/state";
  import { updateInfo, updateModalOpen, updateChecking, updateStatusMessage } from "../stores/update";
  import { checkForUpdates } from "../utils/updater";
  import { raydropApi } from "../utils/tauri-ipc";
  import {
    FolderOpen,
    Terminal,
    Save,
    X,
    CheckCircle2,
    Power,
    RefreshCw,
    Sparkles,
  } from "lucide-svelte";

  let aliasName = "";
  let downloadDir = "";
  let signalingUrl = "";
  let stunAddr = "";
  let autostart = true;
  let win11DirectMenu = false;
  let statusBanner = "";

  $: if ($settingsModalOpen) {
    aliasName = $appSettings.alias_name;
    downloadDir = $appSettings.download_dir;
    signalingUrl = $appSettings.signaling_url;
    stunAddr = $appSettings.stun_addr;
    autostart = $appSettings.autostart ?? true;
    raydropApi.getWin11DirectMenu().then((res) => (win11DirectMenu = res)).catch(() => {});
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
      autostart,
    });
    appSettings.set(updated);
    statusBanner = "Settings saved! Autorun & Signaling configuration updated.";
    setTimeout(() => (statusBanner = ""), 3000);
  }

  async function handleToggleAutostart() {
    try {
      const next = !autostart;
      autostart = await raydropApi.toggleAutostart(next);
      appSettings.update((s) => ({ ...s, autostart }));
      statusBanner = autostart
        ? "Autorun at OS Startup enabled (starts minimized in System Tray)."
        : "Autorun at OS Startup disabled.";
      setTimeout(() => (statusBanner = ""), 3000);
    } catch (e: any) {
      statusBanner = `Autorun error: ${String(e)}`;
    }
  }

  async function handleRegisterContextMenu() {
    try {
      const msg = await raydropApi.registerOsContextMenu();
      statusBanner = msg;
    } catch (e: any) {
      statusBanner = `Context menu error: ${String(e)}`;
    }
  }

  async function handleToggleWin11DirectMenu() {
    try {
      const next = !win11DirectMenu;
      win11DirectMenu = await raydropApi.toggleWin11DirectMenu(next);
      statusBanner = win11DirectMenu
        ? "Direct Right-Click enabled! Explorer refreshed. Raydrop now shows on normal right-click."
        : "Windows 11 modern context menu restored.";
      setTimeout(() => (statusBanner = ""), 4500);
    } catch (e: any) {
      statusBanner = `Windows 11 menu error: ${String(e)}`;
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
              placeholder="wss://stun.raydrop.web.id/ws"
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
              placeholder="stun.raydrop.web.id:3478"
              class="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-xs font-mono text-cyan-300 focus:outline-none focus:border-cyan-500"
            />
          </div>
        </div>

        <!-- Autorun at Startup Toggle -->
        <div
          class="p-3.5 rounded-xl bg-slate-950/90 border border-slate-800 flex items-center justify-between gap-3"
        >
          <div>
            <p
              class="text-xs font-semibold text-slate-200 flex items-center gap-1.5"
            >
              <Power class="w-3.5 h-3.5 text-cyan-400" />
              <span>Autorun at OS Startup</span>
            </p>
            <p class="text-[11px] text-slate-400">
              Automatically launch Raydrop minimized in the System Tray when
              Windows, macOS, or Linux boots up.
            </p>
          </div>
          <button
            type="button"
            on:click={handleToggleAutostart}
            class="px-3 py-2 rounded-xl border text-xs font-semibold flex items-center gap-1.5 shrink-0 transition-all {autostart
              ? 'bg-emerald-500/15 border-emerald-500/40 text-emerald-300'
              : 'bg-slate-800 border-slate-700 text-slate-400'}"
          >
            <span>{autostart ? "Enabled (ON)" : "Disabled (OFF)"}</span>
          </button>
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

        <!-- Windows 11 Direct Right-Click Bypass -->
        <div
          class="p-3.5 rounded-xl bg-slate-950/90 border border-slate-800 flex items-center justify-between gap-3"
        >
          <div>
            <div class="flex items-center gap-2">
              <p class="text-xs font-semibold text-slate-200">
                Windows 11 Direct Right-Click
              </p>
              <span class="text-[9px] font-mono px-1.5 py-0.5 rounded bg-cyan-500/10 text-cyan-300 border border-cyan-500/20">
                No Shift Needed
              </span>
            </div>
            <p class="text-[11px] text-slate-400 mt-0.5">
              Bypasses Windows 11's collapsed "Show more options" so "Send with Raydrop"
              appears immediately on normal right-click.
            </p>
          </div>
          <button
            type="button"
            on:click={handleToggleWin11DirectMenu}
            class="px-3 py-2 rounded-xl border text-xs font-semibold flex items-center gap-1.5 shrink-0 transition-all {win11DirectMenu
              ? 'bg-emerald-500/15 border-emerald-500/40 text-emerald-300 shadow-sm shadow-emerald-500/10'
              : 'bg-slate-800 hover:bg-slate-700 border-slate-700 text-slate-400 hover:text-white'}"
          >
            <span>{win11DirectMenu ? "Enabled (Direct)" : "Disabled (Default Win 11)"}</span>
          </button>
        </div>

        <!-- Software Updates -->
        <div
          class="p-3.5 rounded-xl bg-slate-950/90 border border-slate-800 flex items-center justify-between gap-3"
        >
          <div>
            <div class="flex items-center gap-2">
              <p class="text-xs font-semibold text-slate-200">Software Updates</p>
              <span class="text-[10px] font-mono px-1.5 py-0.5 rounded bg-slate-800 text-slate-400 border border-slate-700">
                Current: v1.1.7
              </span>
            </div>
            <p class="text-[11px] text-slate-400 mt-0.5">
              {#if $updateInfo?.has_update}
                <span class="text-cyan-300 font-medium">New release {$updateInfo.latest_version} available!</span>
              {:else if $updateStatusMessage}
                <span class="text-emerald-400">{$updateStatusMessage}</span>
              {:else}
                Check for updates and automatically upgrade to latest release.
              {/if}
            </p>
          </div>
          <button
            type="button"
            on:click={() => {
              if ($updateInfo?.has_update) {
                updateModalOpen.set(true);
              } else {
                checkForUpdates(true);
              }
            }}
            disabled={$updateChecking}
            class="px-3 py-2 rounded-xl border text-xs font-semibold flex items-center gap-1.5 shrink-0 transition-all {$updateInfo?.has_update
              ? 'bg-cyan-500 hover:bg-cyan-400 text-slate-950 border-cyan-400 shadow-md shadow-cyan-500/20'
              : 'bg-slate-800 hover:bg-slate-700 border-slate-700 text-slate-300'}"
          >
            {#if $updateChecking}
              <RefreshCw class="w-3.5 h-3.5 animate-spin text-cyan-400" />
              <span>Checking...</span>
            {:else if $updateInfo?.has_update}
              <Sparkles class="w-3.5 h-3.5" />
              <span>View Update</span>
            {:else}
              <RefreshCw class="w-3.5 h-3.5" />
              <span>Check Updates</span>
            {/if}
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

