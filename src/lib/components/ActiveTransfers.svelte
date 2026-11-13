<script lang="ts">
  import { activeTransfers, historicalTransfers } from '../stores/transfers';
  import { appSettings } from '../stores/state';
  import { raydropApi, formatBytes, type TransferRecord } from '../utils/tauri-ipc';
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
  } from 'lucide-svelte';

  let resumingId = '';
  let actionError = '';
  let expandedTransferIds: Record<string, boolean> = {};
  let transferFilter: 'all' | 'incoming' | 'outgoing' = 'all';

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
    const seconds = String(date.getSeconds()).padStart(2, '0');
    const timeStr = `${hours}:${minutes}:${seconds}`;

    if (isToday) {
      return `Hari ini, ${timeStr}`;
    }

    const yesterday = new Date();
    yesterday.setDate(now.getDate() - 1);
    if (date.toDateString() === yesterday.toDateString()) {
      return `Kemarin, ${timeStr}`;
    }

    const day = date.getDate();
    const months = ['Jan', 'Feb', 'Mar', 'Apr', 'Mei', 'Jun', 'Jul', 'Agu', 'Sep', 'Okt', 'Nov', 'Des'];
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
    } catch {
      // ignore invalid json
    }
    return [];
  }

  function getBaseName(fullPath: string): string {
    const parts = fullPath.split(/[/\\]/);
    return parts[parts.length - 1] || fullPath;
  }

  function getFallbackFolder(rec: TransferRecord): string | null {
    if (rec.target_path && rec.target_path.trim().length > 0) return rec.target_path;
    if (rec.direction === 'incoming' && $appSettings.download_dir) {
      return $appSettings.download_dir;
    }
    return null;
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

<div class="flex flex-col h-full bg-slate-900/40 border border-slate-800/80 rounded-2xl overflow-hidden">
  <!-- Header Bar -->
  <div class="p-4 border-b border-slate-800/80 flex items-center justify-between gap-2">
    <div class="flex items-center gap-2">
      <Activity class="w-4 h-4 text-cyan-400" />
      <h2 class="text-sm font-bold text-white">Transfers & Streams</h2>
    </div>
    <div class="flex items-center gap-1.5 text-[11px] text-emerald-400 bg-emerald-500/10 px-2.5 py-1 rounded-full border border-emerald-500/20">
      <ShieldCheck class="w-3.5 h-3.5" />
      <span>6x Parallel • Synced</span>
    </div>
  </div>

  <!-- Filter Tabs: All / Received / Sent -->
  <div class="px-4 py-2.5 border-b border-slate-800/80 flex items-center justify-between gap-2 bg-slate-950/60">
    <div class="flex items-center gap-1 bg-slate-950 p-1 rounded-xl border border-slate-800">
      <button
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

    {#if $appSettings.download_dir}
      <button
        on:click={() => handleOpenFolder($appSettings.download_dir)}
        class="flex items-center gap-1.5 text-[11px] text-slate-300 hover:text-white bg-slate-800/90 hover:bg-slate-700/90 px-2.5 py-1 rounded-lg border border-slate-700 transition shrink-0"
        title="Open Downloads Folder ({$appSettings.download_dir})"
      >
        <FolderOpen class="w-3.5 h-3.5 text-amber-400" />
        <span>Downloads Folder</span>
      </button>
    {/if}
  </div>

  <div class="flex-1 overflow-y-auto p-4 space-y-3">
    {#if actionError}
      <div class="p-2.5 rounded-xl bg-rose-500/10 border border-rose-500/30 text-xs text-rose-300 flex items-center justify-between">
        <span>{actionError}</span>
        <button on:click={() => (actionError = '')} class="text-rose-400 hover:text-white ml-2">✕</button>
      </div>
    {/if}

    <!-- Active / Paused Resumable Streaming Transfers -->
    {#if filteredActive.length > 0}
      <div class="space-y-2.5">
        <p class="text-[11px] font-semibold uppercase tracking-wider text-cyan-400">
          Active Transfers ({filteredActive.length})
        </p>
        {#each filteredActive as t (t.id)}
          {@const pct = calcPercent(t.transferred_bytes, t.total_bytes)}
          {@const isParallel = t.total_bytes >= 4 * 1024 * 1024}
          {@const activeFiles = parseFilePaths(t.source_paths)}
          <div class="p-3.5 rounded-xl bg-slate-900 border {t.status === 'paused' ? 'border-amber-500/40' : 'border-cyan-500/30'} space-y-2.5">
            <div class="flex items-center justify-between gap-2">
              <div class="flex items-center gap-1.5 flex-wrap min-w-0">
                {#if t.direction === 'outgoing'}
                  <span class="text-[10px] font-semibold px-2 py-0.5 rounded-full bg-cyan-500/15 text-cyan-300 border border-cyan-500/30 flex items-center gap-1">
                    <ArrowUpRight class="w-3 h-3 text-cyan-400 shrink-0" />
                    Sent to
                  </span>
                {:else}
                  <span class="text-[10px] font-semibold px-2 py-0.5 rounded-full bg-emerald-500/15 text-emerald-300 border border-emerald-500/30 flex items-center gap-1">
                    <ArrowDownLeft class="w-3 h-3 text-emerald-400 shrink-0" />
                    Received from
                  </span>
                {/if}
                <span class="text-xs font-semibold text-white truncate">{t.peer_alias}</span>
                <span class="text-[10px] font-mono uppercase px-1.5 py-0.5 rounded bg-slate-800 text-cyan-300 border border-slate-700">
                  {t.network_route === 'lan_direct' ? 'LAN' : 'WAN P2P'}
                </span>
                {#if isParallel}
                  <span class="text-[10px] font-semibold px-1.5 py-0.5 rounded bg-cyan-500/15 text-cyan-300 border border-cyan-500/30 flex items-center gap-0.5" title="IDM-Style 6-Worker Multi-Segment Transfer">
                    <Zap class="w-2.5 h-2.5" />
                    6x Parallel
                  </span>
                {/if}
                {#if t.status === 'paused'}
                  <span class="text-[10px] font-semibold uppercase px-1.5 py-0.5 rounded bg-amber-500/15 text-amber-300 border border-amber-500/30">
                    Paused / Waiting Resume
                  </span>
                {/if}
              </div>

              <div class="flex items-center gap-2 shrink-0">
                {#if t.status === 'in_progress'}
                  <button
                    on:click={() => handlePause(t.id)}
                    class="text-xs text-amber-300 hover:text-amber-200 bg-amber-500/10 hover:bg-amber-500/20 border border-amber-500/30 px-2 py-1 rounded-lg flex items-center gap-1 transition"
                    title="Pause transfer (can be resumed anytime)"
                  >
                    <Pause class="w-3 h-3" />
                    <span>Pause</span>
                  </button>
                {:else if t.status === 'paused'}
                  {#if t.direction === 'outgoing'}
                    <button
                      on:click={() => handleResume(t.id)}
                      disabled={resumingId === t.id}
                      class="text-xs text-emerald-300 hover:text-emerald-200 bg-emerald-500/15 hover:bg-emerald-500/25 border border-emerald-500/30 px-2.5 py-1 rounded-lg flex items-center gap-1 transition disabled:opacity-50"
                      title="Resume transfer right from the last .raypart byte"
                    >
                      <Play class="w-3 h-3" />
                      <span>{resumingId === t.id ? 'Reconnecting...' : 'Resume'}</span>
                    </button>
                  {:else}
                    <span class="text-[10px] text-amber-300/90 font-mono">
                      Auto-resumes when sender connects
                    </span>
                  {/if}
                {/if}

                <button
                  on:click={() => raydropApi.cancelTransfer(t.id)}
                  class="text-xs text-rose-400 hover:text-rose-300 flex items-center gap-1 px-1.5 py-1"
                  title="Cancel transfer"
                >
                  <XCircle class="w-3.5 h-3.5" />
                </button>
              </div>
            </div>

            <!-- Progress Bar -->
            <div class="w-full h-2 rounded-full bg-slate-950 overflow-hidden border border-slate-800">
              <div
                class="h-full {t.status === 'paused' ? 'bg-amber-400' : 'bg-gradient-to-r from-cyan-400 to-blue-500'} transition-all duration-150"
                style="width: {pct}%"
              ></div>
            </div>

            <div class="flex items-center justify-between text-[11px] font-mono text-slate-400">
              <span>{formatBytes(t.transferred_bytes)} / {formatBytes(t.total_bytes)} ({pct}%)</span>
              {#if t.status === 'paused'}
                <span class="text-amber-300">Saved checkpoint (.raypart) • Ready to resume</span>
              {:else}
                <span>
                  {(t.speed_mbps ?? 0).toFixed(1)} MB/s • ETA {t.eta_seconds ?? 0}s
                </span>
              {/if}
            </div>

            <!-- Transfer Time & Status -->
            <div class="flex items-center justify-between text-[10px] font-mono text-slate-500 pt-1 border-t border-slate-800/60">
              <div class="flex items-center gap-1 text-slate-400">
                <Clock class="w-3 h-3 text-slate-500 shrink-0" />
                <span>Started at: {formatTransferTime(t.created_at)}</span>
              </div>
              <span class="text-cyan-400 font-semibold">{t.total_files} file(s)</span>
            </div>

            {#if activeFiles.length > 0}
              <div class="pt-2 border-t border-slate-800/80 space-y-1.5">
                <p class="text-[10px] font-semibold uppercase tracking-wider text-slate-400">
                  {t.direction === 'incoming' ? 'Receiving Files' : 'Sending Files'} ({activeFiles.length})
                </p>
                <div class="space-y-1 max-h-32 overflow-y-auto pr-1">
                  {#each activeFiles as fp}
                    <div class="flex items-center justify-between gap-2 px-2 py-1.5 rounded-lg bg-slate-950/70 border border-slate-800/80">
                      <div class="flex items-center gap-1.5 min-w-0">
                        <FileText class="w-3.5 h-3.5 text-cyan-400 shrink-0" />
                        <span class="text-xs text-slate-200 truncate" title={fp}>{getBaseName(fp)}</span>
                      </div>
                      <button
                        on:click={() => handleOpenFolder(fp)}
                        class="text-[10px] font-medium text-amber-300 hover:text-white bg-slate-800 hover:bg-slate-700 border border-slate-700 px-2 py-0.5 rounded flex items-center gap-1 shrink-0 transition"
                        title="Open folder & highlight this file ({fp})"
                      >
                        <FolderOpen class="w-3 h-3 text-amber-400" />
                        <span>Open Folder</span>
                      </button>
                    </div>
                  {/each}
                </div>
              </div>
            {/if}
          </div>
        {/each}
      </div>
    {/if}

    <!-- Transfer History -->
    <div class="space-y-2.5">
      <p class="text-[11px] font-semibold uppercase tracking-wider text-slate-400">
        Transfer History ({filteredHistory.length})
      </p>
      {#each filteredHistory as h (h.id)}
        {@const filePaths = parseFilePaths(h.source_paths)}
        {@const fallbackFolder = getFallbackFolder(h)}
        {@const isExpanded = expandedTransferIds[h.id] ?? true}
        {@const visibleFiles = isExpanded ? filePaths : filePaths.slice(0, 3)}
        <div class="p-3.5 rounded-xl bg-slate-900/75 border border-slate-800/90 space-y-2.5">
          <div class="flex items-center justify-between gap-2">
            <div class="flex items-center gap-2.5 min-w-0">
              {#if h.status === 'completed'}
                <CheckCircle2 class="w-4 h-4 text-emerald-400 shrink-0" />
              {:else}
                <AlertTriangle class="w-4 h-4 text-amber-400 shrink-0" />
              {/if}
              <div class="min-w-0">
                <div class="flex items-center gap-1.5 flex-wrap">
                  {#if h.direction === 'outgoing'}
                    <span class="text-[10px] font-semibold px-2 py-0.5 rounded-full bg-cyan-500/15 text-cyan-300 border border-cyan-500/30 flex items-center gap-1">
                      <ArrowUpRight class="w-3 h-3 text-cyan-400 shrink-0" />
                      Sent to
                    </span>
                  {:else}
                    <span class="text-[10px] font-semibold px-2 py-0.5 rounded-full bg-emerald-500/15 text-emerald-300 border border-emerald-500/30 flex items-center gap-1">
                      <ArrowDownLeft class="w-3 h-3 text-emerald-400 shrink-0" />
                      Received from
                    </span>
                  {/if}
                  <span class="text-xs font-semibold text-slate-100 truncate">{h.peer_alias}</span>
                </div>
                <div class="flex items-center gap-2 mt-1 flex-wrap text-[11px] font-mono text-slate-400">
                  <span>{h.total_files} file(s) • {formatBytes(h.transferred_bytes)} / {formatBytes(h.total_bytes)}</span>
                  <span class="text-slate-600">•</span>
                  <div class="flex items-center gap-1 text-slate-400" title="Transfer time">
                    <Clock class="w-3 h-3 text-slate-500 shrink-0" />
                    <span>{formatTransferTime(h.created_at)}</span>
                  </div>
                </div>
              </div>
            </div>
            <div class="flex items-center gap-1.5 shrink-0">
              {#if h.direction === 'outgoing' && (h.status === 'failed' || h.status === 'cancelled') && h.source_paths}
                <button
                  on:click={() => handleResume(h.id)}
                  disabled={resumingId === h.id}
                  class="text-[11px] text-cyan-300 hover:text-cyan-200 bg-cyan-500/10 hover:bg-cyan-500/20 border border-cyan-500/30 px-2 py-0.5 rounded-lg flex items-center gap-1 transition"
                  title="Resume / retry transfer from checkpoint"
                >
                  <RotateCcw class="w-3 h-3" />
                  <span>Resume</span>
                </button>
              {/if}
              <span
                class="text-[10px] font-semibold uppercase px-2 py-0.5 rounded-full {h.status ===
                'completed'
                  ? 'bg-emerald-500/10 text-emerald-300 border border-emerald-500/20'
                  : 'bg-rose-500/10 text-rose-300 border border-rose-500/20'}"
              >
                {h.status}
              </span>
            </div>
          </div>

          <!-- Detailed List of All Files Received / Sent -->
          {#if filePaths.length > 0}
            <div class="pt-2 border-t border-slate-800/80 space-y-1.5">
              <div class="flex items-center justify-between">
                <span class="text-[10px] font-semibold uppercase tracking-wider text-cyan-400">
                  {h.direction === 'incoming' ? 'Received Files' : 'Sent Files'} ({filePaths.length} file(s))
                </span>
                {#if filePaths.length > 3}
                  <button
                    on:click={() => toggleExpanded(h.id)}
                    class="text-[10px] text-slate-400 hover:text-white flex items-center gap-0.5"
                  >
                    {#if isExpanded}
                      <span>Collapse</span>
                      <ChevronUp class="w-3 h-3" />
                    {:else}
                      <span>View All ({filePaths.length})</span>
                      <ChevronDown class="w-3 h-3" />
                    {/if}
                  </button>
                {/if}
              </div>

              <div class="space-y-1.5 max-h-52 overflow-y-auto pr-0.5">
                {#each visibleFiles as fp, idx}
                  <div class="p-2 rounded-lg bg-slate-950/80 border border-slate-800/90 flex items-center justify-between gap-2 hover:border-slate-700 transition">
                    <div class="flex items-center gap-2 min-w-0">
                      <span class="text-[10px] font-mono text-slate-500 w-4 shrink-0">{idx + 1}.</span>
                      <FileText class="w-3.5 h-3.5 text-cyan-400 shrink-0" />
                      <div class="min-w-0">
                        <p class="text-xs font-medium text-slate-100 truncate" title={fp}>
                          {getBaseName(fp)}
                        </p>
                        <p class="text-[10px] font-mono text-slate-500 truncate" title={fp}>
                          {fp}
                        </p>
                      </div>
                    </div>

                    <div class="flex items-center gap-1.5 shrink-0">
                      <button
                        on:click={() => handleOpenFile(fp)}
                        class="text-[11px] font-medium text-cyan-300 hover:text-white bg-cyan-500/15 hover:bg-cyan-500/25 border border-cyan-500/30 px-2 py-1 rounded-md flex items-center gap-1 transition"
                        title="Open this file directly ({getBaseName(fp)})"
                      >
                        <ExternalLink class="w-3 h-3" />
                        <span>Open File</span>
                      </button>
                      <button
                        on:click={() => handleOpenFolder(fp)}
                        class="text-[11px] font-medium text-slate-200 hover:text-white bg-slate-800 hover:bg-slate-700 border border-slate-700 px-2 py-1 rounded-md flex items-center gap-1 transition"
                        title="Open folder & highlight this file ({fp})"
                      >
                        <FolderOpen class="w-3 h-3 text-amber-400" />
                        <span>Open Folder</span>
                      </button>
                    </div>
                  </div>
                {/each}
              </div>
            </div>
          {:else if h.status === 'completed' && fallbackFolder}
            <div class="flex items-center justify-between pt-2 border-t border-slate-800/70">
              <span class="text-[11px] font-mono text-slate-400 truncate" title={fallbackFolder}>
                Location: {fallbackFolder}
              </span>
              <button
                on:click={() => handleOpenFolder(fallbackFolder)}
                class="text-[11px] font-medium text-slate-200 hover:text-white bg-slate-800 hover:bg-slate-700 border border-slate-700 px-2.5 py-1 rounded-lg flex items-center gap-1.5 shrink-0 transition"
                title="Open download folder ({fallbackFolder})"
              >
                <FolderOpen class="w-3 h-3 text-amber-400" />
                <span>Open Folder</span>
              </button>
            </div>
          {/if}
        </div>
      {:else}
        <div class="h-44 flex flex-col items-center justify-center text-center px-4">
          <p class="text-xs text-slate-400">
            {transferFilter === 'incoming'
              ? 'No received files yet.'
              : transferFilter === 'outgoing'
                ? 'No sent files yet.'
                : 'No active or historical transfers.'}
          </p>
          <p class="text-[11px] text-slate-500 mt-1">
            Right-click any file in File Explorer and choose "Send with Raydrop" to transfer files.
          </p>
        </div>
      {/each}
    </div>
  </div>
</div>
