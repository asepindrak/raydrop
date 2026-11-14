<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import QRCode from 'qrcode';
  import {
    raydropApi,
    type WebShareInfo,
  } from '../utils/tauri-ipc';
  import {
    QrCode,
    Smartphone,
    Wifi,
    Copy,
    Check,
    X,
    FileText,
    ExternalLink,
    Download,
    CheckCircle2,
    ShieldCheck,
  } from 'lucide-svelte';

  export let shareInfo: WebShareInfo;
  export let onClose: () => void;

  let qrDataUrl = '';
  let copied = false;
  let activeDownloads: Array<{ file_name: string; peer_ip: string; completed?: boolean }> = [];
  let unlistenStart: (() => void) | null = null;
  let unlistenComplete: (() => void) | null = null;

  onMount(async () => {
    try {
      qrDataUrl = await QRCode.toDataURL(shareInfo.url, {
        width: 260,
        margin: 2,
        color: {
          dark: '#0f172a',
          light: '#ffffff',
        },
      });
    } catch (e) {
      console.error('Failed to generate QR Code:', e);
    }

    try {
      unlistenStart = await raydropApi.listenEvent<{
        peer_ip: string;
        file_id: number;
        file_name: string;
      }>('web_share_download_started', (payload) => {
        activeDownloads = [
          { file_name: payload.file_name, peer_ip: payload.peer_ip, completed: false },
          ...activeDownloads.slice(0, 4),
        ];
      });

      unlistenComplete = await raydropApi.listenEvent<{
        peer_ip: string;
        file_id: number;
        file_name: string;
      }>('web_share_download_completed', (payload) => {
        activeDownloads = activeDownloads.map((d) =>
          d.file_name === payload.file_name ? { ...d, completed: true } : d
        );
      });
    } catch (err) {
      console.warn('Could not attach Web Share listener:', err);
    }
  });

  onDestroy(() => {
    unlistenStart?.();
    unlistenComplete?.();
  });

  async function copyLink() {
    try {
      await navigator.clipboard.writeText(shareInfo.url);
      copied = true;
      setTimeout(() => (copied = false), 2000);
    } catch (err) {
      console.error('Failed to copy link:', err);
    }
  }

  async function handleClose() {
    await raydropApi.stopWebShare();
    onClose();
  }
</script>

<div
  class="fixed inset-0 z-50 bg-slate-950/80 backdrop-blur-md flex items-center justify-center p-4"
  role="dialog"
  aria-modal="true"
>
  <div class="w-full max-w-lg bg-slate-900 border border-cyan-500/30 rounded-2xl p-6 shadow-2xl flex flex-col gap-5 max-h-[90vh] overflow-y-auto">
    <!-- Header -->
    <div class="flex items-center justify-between pb-3 border-b border-slate-800">
      <div class="flex items-center gap-3">
        <div class="w-10 h-10 rounded-xl bg-cyan-500/15 border border-cyan-500/30 flex items-center justify-center text-cyan-400">
          <QrCode class="w-5 h-5" />
        </div>
        <div>
          <h3 class="text-base font-bold text-white flex items-center gap-2">
            <span>Instant Web Share</span>
            <span class="text-[10px] font-mono font-bold px-2 py-0.5 rounded-full bg-cyan-500/15 text-cyan-300 border border-cyan-500/30">
              No App Needed
            </span>
          </h3>
          <p class="text-xs text-slate-400 mt-0.5">
            Scan QR code with your phone or any device on this Wi-Fi
          </p>
        </div>
      </div>
      <button
        on:click={handleClose}
        class="p-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-400 hover:text-white transition-colors"
        title="Close & Stop Sharing"
      >
        <X class="w-4 h-4" />
      </button>
    </div>

    <!-- QR Code & Link Card -->
    <div class="flex flex-col sm:flex-row items-center gap-5 p-4 rounded-xl bg-slate-950/70 border border-slate-800">
      <!-- QR Frame -->
      <div class="p-3 bg-white rounded-2xl shadow-lg shadow-cyan-500/10 shrink-0">
        {#if qrDataUrl}
          <img src={qrDataUrl} alt="Scan QR Code to download" class="w-40 h-40 object-contain block" />
        {:else}
          <div class="w-40 h-40 flex items-center justify-center bg-slate-100 text-slate-400 text-xs">
            Generating QR...
          </div>
        {/if}
      </div>

      <!-- Instructions & Copy Link -->
      <div class="flex-1 flex flex-col justify-between gap-3 min-w-0 w-full">
        <div class="space-y-1.5">
          <div class="flex items-center gap-2 text-xs font-semibold text-slate-200">
            <Smartphone class="w-4 h-4 text-cyan-400 shrink-0" />
            <span>Open Camera on iPhone / Android</span>
          </div>
          <p class="text-[11px] text-slate-400 leading-relaxed">
            Point camera at the QR code and tap the link to download directly in Safari or Chrome.
          </p>
          <div class="flex items-center gap-1.5 text-[11px] text-emerald-400 font-medium pt-1">
            <Wifi class="w-3.5 h-3.5" />
            <span>Must be on the same Wi-Fi network</span>
          </div>
        </div>

        <!-- URL pill -->
        <div>
          <span class="block text-[10px] uppercase font-mono text-slate-500 font-bold mb-1">Direct Browser Link</span>
          <div class="flex items-center gap-1.5 bg-slate-900 border border-slate-700/80 rounded-xl px-2.5 py-1.5">
            <span class="text-xs font-mono text-cyan-300 truncate flex-1">{shareInfo.url}</span>
            <button
              on:click={copyLink}
              class="flex items-center gap-1 px-2 py-1 rounded-lg bg-slate-800 hover:bg-slate-700 text-[11px] font-semibold text-slate-200 transition-colors shrink-0"
              title="Copy URL"
            >
              {#if copied}
                <Check class="w-3 h-3 text-emerald-400" />
                <span class="text-emerald-400">Copied!</span>
              {:else}
                <Copy class="w-3 h-3" />
                <span>Copy</span>
              {/if}
            </button>
          </div>
        </div>
      </div>
    </div>

    <!-- Live Download Activity if any -->
    {#if activeDownloads.length > 0}
      <div class="p-3 rounded-xl bg-cyan-950/30 border border-cyan-500/30 space-y-2">
        <div class="text-[11px] font-bold text-cyan-300 uppercase tracking-wider flex items-center gap-1.5">
          <Download class="w-3.5 h-3.5 animate-pulse" />
          <span>Live Downloads</span>
        </div>
        <div class="space-y-1">
          {#each activeDownloads as act}
            <div class="flex items-center justify-between text-xs text-slate-200 bg-slate-900/60 px-2.5 py-1 rounded-lg">
              <span class="truncate max-w-[240px] font-medium">{act.file_name}</span>
              {#if act.completed}
                <span class="text-[11px] text-emerald-400 font-semibold flex items-center gap-1">
                  <CheckCircle2 class="w-3 h-3" /> Complete
                </span>
              {:else}
                <span class="text-[11px] text-cyan-400 animate-pulse font-mono">Streaming to {act.peer_ip}...</span>
              {/if}
            </div>
          {/each}
        </div>
      </div>
    {/if}

    <!-- Shared Files Breakdown -->
    <div class="space-y-2">
      <div class="flex items-center justify-between text-xs text-slate-400">
        <span class="font-semibold text-slate-300">Shared Files ({shareInfo.total_files})</span>
        <span class="font-mono">{shareInfo.total_bytes_human}</span>
      </div>
      <div class="max-h-40 overflow-y-auto space-y-1 pr-1">
        {#each shareInfo.files as item (item.id)}
          <div class="flex items-center justify-between p-2 rounded-lg bg-slate-950/60 border border-slate-800 text-xs">
            <div class="flex items-center gap-2 min-w-0">
              <FileText class="w-3.5 h-3.5 text-cyan-400 shrink-0" />
              <span class="truncate font-medium text-slate-200">{item.name}</span>
            </div>
            <span class="text-[11px] font-mono text-slate-400 ml-2 shrink-0">{item.size_human}</span>
          </div>
        {/each}
      </div>
    </div>

    <!-- Security Note & Stop Action -->
    <div class="pt-3 border-t border-slate-800 flex items-center justify-between gap-3">
      <div class="flex items-center gap-1.5 text-[11px] text-slate-400">
        <ShieldCheck class="w-3.5 h-3.5 text-emerald-400 shrink-0" />
        <span>100% Direct LAN &bull; 0 server storage</span>
      </div>
      <button
        on:click={handleClose}
        class="px-4 py-2 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-200 font-semibold text-xs transition-colors"
      >
        Done / Stop Sharing
      </button>
    </div>
  </div>
</div>
