<script lang="ts">
  import { open } from '@tauri-apps/plugin-dialog';
  import { activeTransfers, historicalTransfers } from '../stores/transfers';
  import { appSettings, cliQueuedFiles, activeMainTab } from '../stores/state';
  import { contacts, lanContacts } from '../stores/contacts';
  import { raydropApi, formatBytes, type TransferRecord, type WebShareInfo } from '../utils/tauri-ipc';
  import WebShareModal from './WebShareModal.svelte';
  import {
    ArrowUpRight,
    ArrowDownLeft,
    XCircle,
    CheckCircle2,
    AlertTriangle,
    Activity,
    ShieldCheck,
    Pause,
    Play,
    Zap,
    RotateCcw,
    FolderOpen,
    ExternalLink,
    FileText,
    ChevronDown,
    ChevronUp,
    Clock,
    Upload,
    FolderUp,
    Plus,
    Sparkles,
    Send,
    Radio,
    Copy,
    Check,
    QrCode,
  } from 'lucide-svelte';

  let resumingId = '';
  let actionError = '';
  let expandedTransferIds: Record<string, boolean> = {};
  let transferFilter: 'all' | 'incoming' | 'outgoing' = 'all';
  let isDraggingOver = false;
  let webShareInfo: WebShareInfo | null = null;

  function toggleExpanded(id: string) {
    expandedTransferIds = {
      ...expandedTransferIds,
      [id]: !expandedTransferIds[id],
    };
  }

  function calcPercent(transferred: number, total: number): number {
    if (total <= 0) return 0;
    const pct = (transferred / total) * 100;
    if (pct > 0 && pct < 1) return 1;
    return Math.min(100, Math.round(pct));
  }

  function formatTransferTime(unixSecs: number): string {
    if (!unixSecs) return '';
    const date = new Date(unixSecs * 1000);
    const now = new Date();
    const isToday = date.toDateString() === now.toDateString();
    const hours = String(date.getHours()).padStart(2, '0');
    const minutes = String(date.getMinutes()).padStart(2, '0');
    const timeStr = `${hours}:${minutes}`;

    if (isToday) {
      return `Today, ${timeStr}`;
    }

    const yesterday = new Date();
    yesterday.setDate(now.getDate() - 1);
    if (date.toDateString() === yesterday.toDateString()) {
      return `Yesterday, ${timeStr}`;
    }

    const day = date.getDate();
    const months = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
    const month = months[date.getMonth()];
    const year = date.getFullYear();
    return `${day} ${month} ${year}, ${timeStr}`;
  }

  function parseFilePaths(sourcePathsJson?: string | null): string[] {
    if (!sourcePathsJson) return [];
    try {
      const parsed = JSON.parse(sourcePathsJson);
      if (Array.isArray(parsed)) {
        return parsed.filter((x) => typeof x === 'string' && x.trim().length > 0);
      }
      if (typeof parsed === 'string' && parsed.trim().length > 0) {
        return [parsed.trim()];
      }
    } catch {
      if (typeof sourcePathsJson === 'string' && sourcePathsJson.trim().length > 0) {
        return [sourcePathsJson.trim()];
      }
    }
    return [];
  }

  function getBaseName(fullPath: string): string {
    const parts = fullPath.split(/[/\\]/);
    return parts[parts.length - 1] || fullPath;
  }

  function getFileTypeBadge(filename: string): { label: string; color: string } {
    const ext = filename.split('.').pop()?.toUpperCase() || 'FILE';
    if (['SQL', 'DB', 'SQLITE'].includes(ext)) {
      return { label: ext, color: 'bg-emerald-500/20 text-emerald-300 border-emerald-500/40' };
    }
    if (['ZIP', 'RAR', '7Z', 'TAR', 'GZ'].includes(ext)) {
      return { label: ext, color: 'bg-amber-500/20 text-amber-300 border-amber-500/40' };
    }
    if (['MP4', 'MKV', 'MOV', 'AVI', 'WEBM'].includes(ext)) {
      return { label: ext, color: 'bg-purple-500/20 text-purple-300 border-purple-500/40' };
    }
    if (['JPG', 'JPEG', 'PNG', 'WEBP', 'SVG', 'GIF'].includes(ext)) {
      return { label: ext, color: 'bg-pink-500/20 text-pink-300 border-pink-500/40' };
    }
    if (['PDF', 'DOC', 'DOCX', 'XLS', 'XLSX', 'PPTX'].includes(ext)) {
      return { label: ext, color: 'bg-blue-500/20 text-blue-300 border-blue-500/40' };
    }
    if (['ISO', 'EXE', 'MSI', 'DMG', 'BIN'].includes(ext)) {
      return { label: ext, color: 'bg-cyan-500/20 text-cyan-300 border-cyan-500/40' };
    }
    return { label: ext.slice(0, 4), color: 'bg-slate-800 text-slate-300 border-slate-700' };
  }

  $: filteredActive = $activeTransfers.filter((t) => {
    if (transferFilter === 'all') return true;
    return t.direction === transferFilter;
  });

  $: filteredHistory = $historicalTransfers.filter((h) => {
    if (transferFilter === 'all') return true;
    return h.direction === transferFilter;
  });

  $: incomingCount =
    $activeTransfers.filter((t) => t.direction === 'incoming').length +
    $historicalTransfers.filter((h) => h.direction === 'incoming').length;

  $: outgoingCount =
    $activeTransfers.filter((t) => t.direction === 'outgoing').length +
    $historicalTransfers.filter((h) => h.direction === 'outgoing').length;

  $: totalActiveSpeed = $activeTransfers
    .filter((t) => t.status === 'in_progress')
    .reduce((sum, t) => sum + (t.speed_mbps || 0), 0);

  $: onlinePeers = $contacts.filter((c) => c.is_online);

  async function handlePickFiles() {
    try {
      const picked = await open({
        multiple: true,
        title: 'Select Files to Send via Raydrop',
      });
      if (picked) {
        const list = Array.isArray(picked) ? picked : [picked];
        cliQueuedFiles.set(list);
      }
    } catch (e: any) {
      actionError = e?.toString() || 'Failed to select files';
    }
  }

  async function handlePickFolder() {
    try {
      const picked = await open({
        directory: true,
        multiple: false,
        title: 'Select Folder to Send via Raydrop',
      });
      if (typeof picked === 'string') {
        cliQueuedFiles.set([picked]);
      }
    } catch (e: any) {
      actionError = e?.toString() || 'Failed to select folder';
    }
  }

  async function handleShareViaWeb() {
    try {
      const picked = await open({
        multiple: true,
        title: 'Select Files or Folder to Share to Phone / Web',
      });
      if (picked) {
        const list = Array.isArray(picked) ? picked : [picked];
        const info = await raydropApi.startWebShare(list);
        webShareInfo = info;
      }
    } catch (e: any) {
      actionError = e?.toString() || 'Failed to start Web Share';
    }
  }

  async function handleQuickSendToPeer(peerDeviceId: string) {
    try {
      const picked = await open({
        multiple: true,
        title: 'Select Files to Send',
      });
      if (picked) {
        const list = Array.isArray(picked) ? picked : [picked];
        await raydropApi.initiateTransfer(peerDeviceId, list);
      }
    } catch (e: any) {
      actionError = e?.toString() || 'Failed to send to peer';
    }
  }

  async function handleOpenFile(path: string) {
    actionError = '';
    try {
      await raydropApi.openFilePath(path);
    } catch (e: any) {
      actionError = e?.toString() || 'Failed to open file';
    }
  }

  async function handleOpenFolder(path: string) {
    actionError = '';
    try {
      await raydropApi.openFolderPath(path);
    } catch (e: any) {
      actionError = e?.toString() || 'Failed to open folder';
    }
  }

  async function handlePause(sessionId: string) {
    actionError = '';
    try {
      await raydropApi.pauseTransfer(sessionId);
    } catch (e: any) {
      actionError = e?.toString() || 'Failed to pause transfer';
    }
  }

  async function handleResume(sessionId: string) {
    actionError = '';
    resumingId = sessionId;
    try {
      await raydropApi.resumeTransfer(sessionId);
    } catch (e: any) {
      actionError = e?.toString() || 'Failed to resume transfer';
    } finally {
      resumingId = '';
    }
  }
</script>

<div class="flex flex-col h-full bg-slate-900/40 border border-slate-800/80 rounded-2xl overflow-hidden backdrop-blur-xl">
  <!-- Top Hub: Quick Transfer Action Bar & Drop Zone -->
  <div class="p-5 border-b border-slate-800/80 bg-slate-950/60 flex flex-col lg:flex-row items-stretch lg:items-center justify-between gap-4">
    <!-- Action Buttons -->
    <div class="flex items-center gap-3 shrink-0">
      <button
        type="button"
        on:click={handlePickFiles}
        class="flex items-center gap-2 px-5 py-2.5 rounded-xl bg-gradient-to-r from-cyan-500 to-indigo-600 hover:from-cyan-400 hover:to-indigo-500 text-white font-bold text-sm shadow-lg shadow-cyan-500/25 transition-all transform hover:-translate-y-0.5 active:translate-y-0"
      >
        <Plus class="w-4 h-4 stroke-[2.5]" />
        <span>Send Files</span>
      </button>

      <button
        type="button"
        on:click={handlePickFolder}
        class="flex items-center gap-2 px-4 py-2.5 rounded-xl bg-slate-800/80 hover:bg-slate-700 text-slate-200 hover:text-white font-semibold text-sm border border-slate-700/80 transition-all hover:-translate-y-0.5"
      >
        <FolderUp class="w-4 h-4 text-cyan-400" />
        <span>Send Folder</span>
      </button>

      <button
        type="button"
        on:click={handleShareViaWeb}
        class="flex items-center gap-2 px-4 py-2.5 rounded-xl bg-gradient-to-r from-purple-600/80 to-pink-600/80 hover:from-purple-500 hover:to-pink-500 text-white font-semibold text-sm border border-purple-500/30 transition-all hover:-translate-y-0.5 shadow-lg shadow-purple-500/20"
        title="Share files to phone or any device via QR code (no app needed)"
      >
        <QrCode class="w-4 h-4 text-purple-200" />
        <span>Share to Phone</span>
      </button>
    </div>

    <!-- Quick Drop & Online Peers Hub -->
    <button
      type="button"
      on:click={handlePickFiles}
      class="flex-1 max-w-2xl px-4 py-2.5 rounded-xl border border-dashed border-slate-700/80 hover:border-cyan-500/60 bg-slate-900/40 hover:bg-slate-900/80 transition-all cursor-pointer flex items-center justify-between gap-3 text-xs text-slate-400 group text-left"
      title="Click or drag files here to send"
    >
      <div class="flex items-center gap-2.5 min-w-0">
        <div class="w-7 h-7 rounded-lg bg-cyan-500/10 group-hover:bg-cyan-500/20 text-cyan-400 flex items-center justify-center shrink-0 transition-colors">
          <Upload class="w-4 h-4" />
        </div>
        <div class="truncate">
          <span class="text-slate-200 font-semibold group-hover:text-cyan-300 transition-colors">
            Drop files or folders here
          </span>
          <span class="text-slate-500 hidden sm:inline"> — Zero Cloud Storage • Encrypted P2P</span>
        </div>
      </div>

      <div class="flex items-center gap-1.5 shrink-0">
        <span class="px-2 py-0.5 rounded-full bg-slate-800/80 border border-slate-700 text-[10px] font-mono text-cyan-400">
          Unlimited Size
        </span>
      </div>
    </button>

    <!-- Quick Online Peers Shortcut -->
    {#if onlinePeers.length > 0}
      <div class="hidden xl:flex items-center gap-1.5 shrink-0 text-xs">
        <span class="text-slate-500 font-medium">Quick Send:</span>
        {#each onlinePeers.slice(0, 2) as peer}
          <button
            type="button"
            on:click={() => handleQuickSendToPeer(peer.device_id)}
            class="flex items-center gap-1.5 px-2.5 py-1 rounded-lg bg-slate-800/70 hover:bg-slate-700 border border-slate-700 text-slate-300 hover:text-white transition-colors"
            title="Send files directly to {peer.alias_name}"
          >
            <span class="w-2 h-2 rounded-full bg-emerald-400 animate-pulse"></span>
            <span class="font-semibold truncate max-w-[100px]">{peer.alias_name}</span>
          </button>
        {/each}
      </div>
    {/if}
  </div>

  <!-- Filter Ribbon & Aggregate Metrics -->
  <div class="px-5 py-3 border-b border-slate-800/80 flex items-center justify-between gap-3 bg-slate-950/40">
    <!-- Filter Tabs: All / Received / Sent -->
    <div class="flex items-center gap-1 bg-slate-950 p-1 rounded-xl border border-slate-800">
      <button
        type="button"
        on:click={() => (transferFilter = 'all')}
        class="flex items-center gap-1.5 px-3 py-1 rounded-lg text-xs font-semibold transition-all {transferFilter === 'all'
          ? 'bg-cyan-500/20 text-cyan-300 border border-cyan-500/30'
          : 'text-slate-400 hover:text-slate-200'}"
      >
        <span>All</span>
        <span class="text-[10px] px-1.5 py-0.2 rounded-full bg-slate-800 text-slate-300 font-mono">
          {$activeTransfers.length + $historicalTransfers.length}
        </span>
      </button>

      <button
        type="button"
        on:click={() => (transferFilter = 'incoming')}
        class="flex items-center gap-1.5 px-3 py-1 rounded-lg text-xs font-semibold transition-all {transferFilter === 'incoming'
          ? 'bg-emerald-500/20 text-emerald-300 border border-emerald-500/30'
          : 'text-slate-400 hover:text-slate-200'}"
      >
        <ArrowDownLeft class="w-3.5 h-3.5 text-emerald-400" />
        <span>Received</span>
        <span class="text-[10px] px-1.5 py-0.2 rounded-full bg-slate-800 text-emerald-400 font-mono">
          {incomingCount}
        </span>
      </button>

      <button
        type="button"
        on:click={() => (transferFilter = 'outgoing')}
        class="flex items-center gap-1.5 px-3 py-1 rounded-lg text-xs font-semibold transition-all {transferFilter === 'outgoing'
          ? 'bg-cyan-500/20 text-cyan-300 border border-cyan-500/30'
          : 'text-slate-400 hover:text-slate-200'}"
      >
        <ArrowUpRight class="w-3.5 h-3.5 text-cyan-400" />
        <span>Sent</span>
        <span class="text-[10px] px-1.5 py-0.2 rounded-full bg-slate-800 text-cyan-300 font-mono">
          {outgoingCount}
        </span>
      </button>
    </div>

    <!-- Live Performance Badge -->
    <div class="flex items-center gap-3">
      {#if totalActiveSpeed > 0}
        <div class="flex items-center gap-2 px-3 py-1 rounded-full bg-emerald-500/15 border border-emerald-500/30 text-emerald-300 text-xs font-mono font-bold animate-pulse">
          <Zap class="w-3.5 h-3.5 text-emerald-400" />
          <span>{totalActiveSpeed.toFixed(1)} MB/s Active</span>
        </div>
      {:else}
        <div class="hidden sm:flex items-center gap-1.5 text-xs text-slate-400 bg-slate-900/60 px-3 py-1 rounded-full border border-slate-800">
          <ShieldCheck class="w-3.5 h-3.5 text-cyan-400" />
          <span>ChaCha20-Poly1305 + BLAKE3</span>
        </div>
      {/if}

      {#if $appSettings.download_dir}
        <button
          type="button"
          on:click={() => handleOpenFolder($appSettings.download_dir)}
          class="flex items-center gap-1.5 text-xs text-slate-300 hover:text-white bg-slate-800/90 hover:bg-slate-700 px-3 py-1 rounded-xl border border-slate-700 transition shrink-0"
          title="Open Downloads Folder ({$appSettings.download_dir})"
        >
          <FolderOpen class="w-3.5 h-3.5 text-amber-400" />
          <span>Downloads Folder</span>
        </button>
      {/if}
    </div>
  </div>

  <!-- Content: Active Transfers & History (Full View) -->
  <div class="flex-1 overflow-y-auto p-5 space-y-6">
    {#if actionError}
      <div class="p-3 rounded-xl bg-rose-500/10 border border-rose-500/30 text-xs text-rose-300 flex items-center justify-between">
        <span>{actionError}</span>
        <button on:click={() => (actionError = '')} class="text-rose-400 hover:text-white ml-2">✕</button>
      </div>
    {/if}

    <!-- 1. Active / Paused Resumable Streaming Transfers -->
    {#if filteredActive.length > 0}
      <div class="space-y-3">
        <div class="flex items-center justify-between">
          <div class="flex items-center gap-2">
            <Activity class="w-4 h-4 text-cyan-400 animate-pulse" />
            <h3 class="text-xs font-bold uppercase tracking-wider text-cyan-400">
              Active Transfers ({filteredActive.length})
            </h3>
          </div>
          <span class="text-xs text-slate-400 font-mono">Real-time Stream Engine</span>
        </div>

        <div class="grid grid-cols-1 xl:grid-cols-2 gap-3.5">
          {#each filteredActive as t (t.id)}
            {@const pct = calcPercent(t.transferred_bytes, t.total_bytes)}
            {@const isParallel = t.total_bytes >= 4 * 1024 * 1024}
            {@const activeFiles = parseFilePaths(t.source_paths)}
            {@const primaryName = activeFiles[0] ? getBaseName(activeFiles[0]) : 'Transfer Stream'}
            {@const badge = getFileTypeBadge(primaryName)}

            <div class="p-4 rounded-2xl bg-slate-900/90 border {t.status === 'paused' ? 'border-amber-500/40 shadow-amber-500/5' : 'border-cyan-500/30 shadow-cyan-500/5'} shadow-xl space-y-3 transition-all">
              <div class="flex items-start justify-between gap-3">
                <div class="flex items-center gap-3 min-w-0">
                  <!-- File Type Icon Box (Matching Landing Page Demo Window) -->
                  <div class="w-11 h-11 rounded-xl {badge.color} border flex items-center justify-center font-mono font-extrabold text-xs shrink-0 shadow-sm">
                    {badge.label}
                  </div>

                  <div class="min-w-0">
                    <h4 class="text-sm font-bold text-white truncate" title={primaryName}>
                      {primaryName}
                      {#if activeFiles.length > 1}
                        <span class="text-xs font-normal text-slate-400">(+{activeFiles.length - 1} more)</span>
                      {/if}
                    </h4>

                    <div class="flex items-center gap-2 mt-0.5 text-xs text-slate-400">
                      {#if t.direction === 'outgoing'}
                        <span class="text-cyan-300 font-semibold flex items-center gap-1">
                          <ArrowUpRight class="w-3.5 h-3.5 text-cyan-400" />
                          To: {t.peer_alias}
                        </span>
                      {:else}
                        <span class="text-emerald-300 font-semibold flex items-center gap-1">
                          <ArrowDownLeft class="w-3.5 h-3.5 text-emerald-400" />
                          From: {t.peer_alias}
                        </span>
                      {/if}

                      <span>•</span>
                      <span class="text-[10px] font-mono uppercase px-1.5 py-0.2 rounded bg-slate-800 text-cyan-300 border border-slate-700">
                        {t.network_route === 'lan_direct' ? 'LAN Direct' : 'WAN P2P'}
                      </span>

                      {#if isParallel}
                        <span class="text-[10px] font-semibold px-1.5 py-0.2 rounded bg-cyan-500/15 text-cyan-300 border border-cyan-500/30 flex items-center gap-0.5">
                          <Zap class="w-2.5 h-2.5" />
                          6x Parallel
                        </span>
                      {/if}
                    </div>
                  </div>
                </div>

                <!-- Action Controls -->
                <div class="flex items-center gap-2 shrink-0">
                  {#if t.status === 'in_progress'}
                    <button
                      type="button"
                      on:click={() => handlePause(t.id)}
                      class="text-xs text-amber-300 hover:text-amber-200 bg-amber-500/10 hover:bg-amber-500/20 border border-amber-500/30 px-2.5 py-1 rounded-xl flex items-center gap-1 transition"
                      title="Pause transfer (resumable anytime)"
                    >
                      <Pause class="w-3.5 h-3.5" />
                      <span>Pause</span>
                    </button>
                  {:else if t.status === 'paused'}
                    {#if t.direction === 'outgoing'}
                      <button
                        type="button"
                        on:click={() => handleResume(t.id)}
                        disabled={resumingId === t.id}
                        class="text-xs text-emerald-300 hover:text-emerald-200 bg-emerald-500/15 hover:bg-emerald-500/25 border border-emerald-500/30 px-3 py-1 rounded-xl flex items-center gap-1 transition disabled:opacity-50"
                        title="Resume transfer from the last byte"
                      >
                        <Play class="w-3.5 h-3.5" />
                        <span>{resumingId === t.id ? 'Reconnecting...' : 'Resume'}</span>
                      </button>
                    {:else}
                      <span class="text-[10px] text-amber-300/90 font-mono bg-amber-500/10 px-2 py-0.5 rounded-lg border border-amber-500/20">
                        Waiting for sender...
                      </span>
                    {/if}
                  {/if}

                  <button
                    type="button"
                    on:click={() => raydropApi.cancelTransfer(t.id)}
                    class="p-1.5 text-rose-400 hover:text-rose-300 hover:bg-rose-500/10 rounded-xl transition"
                    title="Cancel transfer"
                  >
                    <XCircle class="w-4 h-4" />
                  </button>
                </div>
              </div>

              <!-- Progress Bar -->
              <div class="space-y-1.5">
                <div class="w-full h-2.5 rounded-full bg-slate-950 overflow-hidden border border-slate-800">
                  <div
                    class="h-full {t.status === 'paused'
                      ? 'bg-amber-400'
                      : 'bg-gradient-to-r from-cyan-400 via-blue-500 to-indigo-500'} transition-all duration-150"
                    style="width: {pct}%"
                  ></div>
                </div>

                <div class="flex items-center justify-between text-xs font-mono text-slate-400">
                  <span>{formatBytes(t.transferred_bytes)} / {formatBytes(t.total_bytes)} ({pct}%)</span>
                  {#if t.status === 'paused'}
                    <span class="text-amber-300">Checkpoint saved (.raypart) • Ready to resume</span>
                  {:else}
                    <span class="text-emerald-400 font-bold">
                      {(t.speed_mbps ?? 0).toFixed(1)} MB/s • ETA {t.eta_seconds ?? 0}s
                    </span>
                  {/if}
                </div>
              </div>

              <!-- Metadata & File Details Toggle -->
              <div class="flex items-center justify-between text-[11px] font-mono text-slate-500 pt-2 border-t border-slate-800/60">
                <div class="flex items-center gap-1.5 text-slate-400">
                  <Clock class="w-3 h-3 text-slate-500 shrink-0" />
                  <span>Started: {formatTransferTime(t.created_at)}</span>
                </div>

                <div class="flex items-center gap-2">
                  <span class="text-cyan-400 font-semibold">{t.total_files} file(s)</span>
                  {#if activeFiles.length > 0}
                    <button
                      type="button"
                      on:click={() => toggleExpanded(t.id)}
                      class="text-slate-400 hover:text-slate-200 flex items-center gap-0.5"
                    >
                      <span>{expandedTransferIds[t.id] ? 'Hide' : 'Files'}</span>
                      {#if expandedTransferIds[t.id]}
                        <ChevronUp class="w-3 h-3" />
                      {:else}
                        <ChevronDown class="w-3 h-3" />
                      {/if}
                    </button>
                  {/if}
                </div>
              </div>

              <!-- Expanded File List -->
              {#if expandedTransferIds[t.id] && activeFiles.length > 0}
                <div class="pt-2 border-t border-slate-800/80 space-y-1.5 max-h-40 overflow-y-auto pr-1">
                  {#each activeFiles as fp}
                    <div class="flex items-center justify-between gap-2 px-2.5 py-1.5 rounded-lg bg-slate-950/80 border border-slate-800">
                      <div class="flex items-center gap-2 min-w-0">
                        <FileText class="w-3.5 h-3.5 text-cyan-400 shrink-0" />
                        <span class="text-xs text-slate-200 truncate" title={fp}>{getBaseName(fp)}</span>
                      </div>
                      <button
                        type="button"
                        on:click={() => handleOpenFolder(fp)}
                        class="text-[11px] text-cyan-400 hover:underline shrink-0"
                      >
                        Locate
                      </button>
                    </div>
                  {/each}
                </div>
              {/if}
            </div>
          {/each}
        </div>
      </div>
    {/if}

    <!-- 2. Transfer History (Completed Transfers) -->
    <div class="space-y-3">
      <div class="flex items-center justify-between">
        <h3 class="text-xs font-bold uppercase tracking-wider text-slate-400">
          Transfer History ({filteredHistory.length})
        </h3>
        {#if filteredHistory.length > 0}
          <button
            type="button"
            on:click={async () => {
              await raydropApi.clearTransferHistory();
              historicalTransfers.set([]);
            }}
            class="text-[11px] text-slate-500 hover:text-rose-400 transition"
          >
            Clear History
          </button>
        {/if}
      </div>

      {#if filteredHistory.length === 0 && filteredActive.length === 0}
        <!-- Beautiful Empty State (Matching Landing Page Demo Aesthetic) -->
        <div class="h-80 flex flex-col items-center justify-center text-center p-8 border border-dashed border-slate-800 rounded-2xl bg-slate-950/30">
          <div class="w-16 h-16 rounded-2xl bg-cyan-500/10 border border-cyan-500/30 flex items-center justify-center mb-4 text-cyan-400 shadow-xl shadow-cyan-500/10">
            <Upload class="w-8 h-8 stroke-[1.8]" />
          </div>
          <h4 class="text-base font-bold text-white font-display">Ready for High-Speed P2P Transfers</h4>
          <p class="text-xs text-slate-400 mt-1.5 max-w-md leading-relaxed">
            Send files and folders of any size directly between computers on your local network (LAN) or internet (WAN). Zero cloud servers involved.
          </p>

          <div class="flex items-center gap-3 mt-5">
            <button
              type="button"
              on:click={handlePickFiles}
              class="flex items-center gap-2 px-5 py-2.5 rounded-xl bg-gradient-to-r from-cyan-500 to-indigo-600 hover:from-cyan-400 hover:to-indigo-500 text-white font-bold text-xs shadow-lg shadow-cyan-500/20 transition-all"
            >
              <Plus class="w-4 h-4" />
              <span>Select Files to Send</span>
            </button>

            <button
              type="button"
              on:click={() => activeMainTab.set('lan')}
              class="flex items-center gap-2 px-4 py-2.5 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs font-semibold border border-slate-700 transition"
            >
              <Radio class="w-4 h-4 text-cyan-400" />
              <span>Scan LAN Radar</span>
            </button>

            <button
              type="button"
              on:click={handleShareViaWeb}
              class="flex items-center gap-2 px-4 py-2.5 rounded-xl bg-purple-600/20 hover:bg-purple-600/30 text-purple-200 text-xs font-semibold border border-purple-500/40 transition"
            >
              <QrCode class="w-4 h-4 text-purple-300" />
              <span>Share to Phone (QR Code)</span>
            </button>
          </div>
        </div>
      {:else if filteredHistory.length === 0}
        <p class="text-xs text-slate-500 italic p-4 text-center">No completed transfers match the filter.</p>
      {:else}
        <div class="space-y-2">
          {#each filteredHistory as h (h.id)}
            {@const files = parseFilePaths(h.source_paths)}
            {@const primaryFilePath = files[0] || h.target_path}
            {@const primaryName = files[0] ? getBaseName(files[0]) : (h.target_path ? getBaseName(h.target_path) : 'File Package')}
            {@const badge = getFileTypeBadge(primaryName)}

            <div class="p-3.5 rounded-xl bg-slate-900/70 hover:bg-slate-900 border border-slate-800/90 hover:border-slate-700 flex flex-col gap-2.5 transition-colors">
              <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-3">
                <div class="flex items-center gap-3 min-w-0">
                  <div class="w-9 h-9 rounded-xl {badge.color} border flex items-center justify-center font-mono font-bold text-[10px] shrink-0">
                    {badge.label}
                  </div>

                  <div class="min-w-0">
                    <div class="flex items-center gap-2">
                      <span class="text-xs font-bold text-white truncate max-w-sm" title={primaryName}>
                        {primaryName}
                      </span>
                      {#if files.length > 1}
                        <button
                          type="button"
                          on:click={() => toggleExpanded(h.id)}
                          class="text-[10px] text-cyan-400 hover:text-cyan-300 font-medium flex items-center gap-0.5"
                        >
                          <span>({files.length} files)</span>
                          {#if expandedTransferIds[h.id]}
                            <ChevronUp class="w-3 h-3" />
                          {:else}
                            <ChevronDown class="w-3 h-3" />
                          {/if}
                        </button>
                      {/if}
                      <span class="text-[10px] font-mono text-slate-400">({formatBytes(h.total_bytes)})</span>
                    </div>

                    <div class="flex items-center gap-2 mt-0.5 text-[11px] text-slate-400">
                      {#if h.direction === 'outgoing'}
                        <span class="text-cyan-300 font-medium flex items-center gap-1">
                          <ArrowUpRight class="w-3 h-3 text-cyan-400" />
                          Sent to {h.peer_alias}
                        </span>
                      {:else}
                        <span class="text-emerald-300 font-medium flex items-center gap-1">
                          <ArrowDownLeft class="w-3 h-3 text-emerald-400" />
                          Received from {h.peer_alias}
                        </span>
                      {/if}
                      <span>•</span>
                      <span class="text-slate-500">{formatTransferTime(h.created_at)}</span>
                    </div>
                  </div>
                </div>

                <!-- Quick Action Buttons -->
                <div class="flex items-center gap-2 shrink-0 self-end sm:self-center">
                  {#if h.status === 'completed'}
                    <span class="flex items-center gap-1 text-[11px] text-emerald-400 font-semibold bg-emerald-500/10 px-2 py-0.5 rounded-full border border-emerald-500/20">
                      <CheckCircle2 class="w-3.5 h-3.5" />
                      Verified
                    </span>

                    {#if primaryFilePath}
                      <button
                        type="button"
                        on:click={() => handleOpenFile(primaryFilePath)}
                        class="px-2.5 py-1 rounded-lg bg-slate-800 hover:bg-slate-700 text-xs text-slate-200 hover:text-white border border-slate-700 transition"
                        title="Open File Directly"
                      >
                        Open
                      </button>
                      <button
                        type="button"
                        on:click={() => handleOpenFolder(primaryFilePath)}
                        class="p-1 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-400 hover:text-amber-300 border border-slate-700 transition"
                        title="Open Containing Folder"
                      >
                        <FolderOpen class="w-3.5 h-3.5" />
                      </button>
                    {/if}
                  {:else if h.status === 'failed'}
                    <span class="flex items-center gap-1 text-[11px] text-rose-400 font-semibold bg-rose-500/10 px-2 py-0.5 rounded-full border border-rose-500/20">
                      <XCircle class="w-3.5 h-3.5" />
                      Failed
                    </span>
                  {:else if h.status === 'cancelled'}
                    <span class="flex items-center gap-1 text-[11px] text-slate-400 font-semibold bg-slate-800 px-2 py-0.5 rounded-full border border-slate-700">
                      Cancelled
                    </span>
                  {/if}
                </div>
              </div>

              <!-- Expanded Multiple Files List for History -->
              {#if expandedTransferIds[h.id] && files.length > 1}
                <div class="pt-2 border-t border-slate-800/80 space-y-1.5 max-h-40 overflow-y-auto pr-1">
                  {#each files as fp}
                    <div class="flex items-center justify-between gap-2 px-2.5 py-1.5 rounded-lg bg-slate-950/80 border border-slate-800">
                      <div class="flex items-center gap-2 min-w-0">
                        <FileText class="w-3.5 h-3.5 text-cyan-400 shrink-0" />
                        <span class="text-xs text-slate-200 truncate" title={fp}>{getBaseName(fp)}</span>
                      </div>
                      <div class="flex items-center gap-2 shrink-0">
                        <button
                          type="button"
                          on:click={() => handleOpenFile(fp)}
                          class="text-[11px] text-cyan-400 hover:text-cyan-300 hover:underline"
                        >
                          Open
                        </button>
                        <button
                          type="button"
                          on:click={() => handleOpenFolder(fp)}
                          class="text-[11px] text-slate-400 hover:text-slate-200 hover:underline"
                        >
                          Locate
                        </button>
                      </div>
                    </div>
                  {/each}
                </div>
              {/if}
            </div>
          {/each}
        </div>
      {/if}
    </div>
  </div>

  {#if webShareInfo}
    <WebShareModal
      shareInfo={webShareInfo}
      onClose={() => {
        webShareInfo = null;
      }}
    />
  {/if}
</div>
