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
  } from 'lucide-svelte';

  let resumingId = '';
  let actionError = '';

  function calcPercent(transferred: number, total: number): number {
    if (total <= 0) return 0;
    return Math.min(100, Math.round((transferred / total) * 100));
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

  function getPrimaryFilePath(rec: TransferRecord): string | null {
    const paths = parseFilePaths(rec.source_paths);
    return paths.length > 0 ? paths[0] : null;
  }

  function getFolderTarget(rec: TransferRecord): string | null {
    const primaryFile = getPrimaryFilePath(rec);
    if (primaryFile) return primaryFile;
    if (rec.target_path && rec.target_path.trim().length > 0) return rec.target_path;
    if (rec.direction === 'incoming' && $appSettings.download_dir) {
      return $appSettings.download_dir;
    }
    return null;
  }

  async function handleOpenFile(path: string) {
    actionError = '';
    try {
      await raydropApi.openFilePath(path);
    } catch (e: any) {
      actionError = e?.toString() || 'Gagal membuka file';
    }
  }

  async function handleOpenFolder(path: string) {
    actionError = '';
    try {
      await raydropApi.openFolderPath(path);
    } catch (e: any) {
      actionError = e?.toString() || 'Gagal membuka folder';
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
  <div class="p-4 border-b border-slate-800/80 flex items-center justify-between gap-2">
    <div class="flex items-center gap-2">
      <Activity class="w-4 h-4 text-cyan-400" />
      <h2 class="text-sm font-bold text-white">Transfers & Encrypted Streams</h2>
    </div>
    <div class="flex items-center gap-2">
      {#if $appSettings.download_dir}
        <button
          on:click={() => handleOpenFolder($appSettings.download_dir)}
          class="flex items-center gap-1.5 text-[11px] text-slate-300 hover:text-white bg-slate-800/90 hover:bg-slate-700/90 px-2.5 py-1 rounded-full border border-slate-700 transition"
          title="Buka Folder Download ({$appSettings.download_dir})"
        >
          <FolderOpen class="w-3.5 h-3.5 text-cyan-400" />
          <span>Folder Download</span>
        </button>
      {/if}
      <div class="flex items-center gap-1.5 text-[11px] text-emerald-400 bg-emerald-500/10 px-2.5 py-1 rounded-full border border-emerald-500/20">
        <ShieldCheck class="w-3.5 h-3.5" />
        <span>6x Parallel • E2EE</span>
      </div>
    </div>
  </div>

  <div class="flex-1 overflow-y-auto p-4 space-y-3">
    {#if actionError}
      <div class="p-2.5 rounded-xl bg-rose-500/10 border border-rose-500/30 text-xs text-rose-300 flex items-center justify-between">
        <span>{actionError}</span>
        <button on:click={() => (actionError = '')} class="text-rose-400 hover:text-white ml-2">✕</button>
      </div>
    {/if}

    <!-- Active / Paused Resumable Streaming Transfers -->
    {#if $activeTransfers.length > 0}
      <div class="space-y-2.5">
        <p class="text-[11px] font-semibold uppercase tracking-wider text-cyan-400">
          Active & Resumable Streams ({$activeTransfers.length})
        </p>
        {#each $activeTransfers as t (t.id)}
          {@const pct = calcPercent(t.transferred_bytes, t.total_bytes)}
          {@const isParallel = t.total_bytes >= 4 * 1024 * 1024}
          {@const activeFile = getPrimaryFilePath(t)}
          <div class="p-3.5 rounded-xl bg-slate-900 border {t.status === 'paused' ? 'border-amber-500/40' : 'border-cyan-500/30'} space-y-2.5">
            <div class="flex items-center justify-between gap-2">
              <div class="flex items-center gap-1.5 flex-wrap min-w-0">
                {#if t.direction === 'outgoing'}
                  <ArrowUpRight class="w-4 h-4 text-cyan-400 shrink-0" />
                {:else}
                  <ArrowDownLeft class="w-4 h-4 text-emerald-400 shrink-0" />
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
                      title="Continue transfer from exact .raypart byte offset"
                    >
                      <Play class="w-3 h-3" />
                      <span>{resumingId === t.id ? 'Reconnecting...' : 'Resume'}</span>
                    </button>
                  {:else}
                    <span class="text-[10px] text-amber-300/90 font-mono">
                      Auto-resumes when sender reconnects
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

            {#if activeFile}
              <p class="text-xs font-medium text-cyan-300 truncate flex items-center gap-1" title={activeFile}>
                <FileText class="w-3 h-3 shrink-0" />
                <span class="truncate">{getBaseName(activeFile)}</span>
              </p>
            {/if}

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
          </div>
        {/each}
      </div>
    {/if}

    <!-- Transfer History -->
    <div class="space-y-2">
      <p class="text-[11px] font-semibold uppercase tracking-wider text-slate-400">
        Recent History ({$historicalTransfers.length})
      </p>
      {#each $historicalTransfers as h (h.id)}
        {@const filePaths = parseFilePaths(h.source_paths)}
        {@const primaryFile = getPrimaryFilePath(h)}
        {@const folderTarget = getFolderTarget(h)}
        <div class="p-3 rounded-xl bg-slate-900/70 border border-slate-800/80 space-y-2">
          <div class="flex items-center justify-between gap-2">
            <div class="flex items-center gap-2.5 min-w-0">
              {#if h.status === 'completed'}
                <CheckCircle2 class="w-4 h-4 text-emerald-400 shrink-0" />
              {:else}
                <AlertTriangle class="w-4 h-4 text-amber-400 shrink-0" />
              {/if}
              <div class="min-w-0">
                <div class="flex items-center gap-1.5">
                  <span class="text-xs font-medium text-slate-200 truncate">{h.peer_alias}</span>
                  <span class="text-[10px] font-mono text-slate-500">
                    ({h.direction === 'outgoing' ? 'Sent' : 'Received'})
                  </span>
                </div>
                {#if primaryFile}
                  <p class="text-xs font-medium text-cyan-300 truncate flex items-center gap-1 mt-0.5" title={primaryFile}>
                    <FileText class="w-3 h-3 shrink-0" />
                    <span class="truncate">{getBaseName(primaryFile)}{filePaths.length > 1 ? ` (+${filePaths.length - 1} more)` : ''}</span>
                  </p>
                {/if}
                <p class="text-[11px] font-mono text-slate-400">
                  {h.total_files} file(s) • {formatBytes(h.transferred_bytes)} / {formatBytes(h.total_bytes)}
                </p>
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

          {#if h.status === 'completed' && (primaryFile || folderTarget)}
            <div class="flex items-center gap-2 pt-1.5 border-t border-slate-800/70">
              {#if primaryFile}
                <button
                  on:click={() => handleOpenFile(primaryFile)}
                  class="text-[11px] font-medium text-cyan-300 hover:text-white bg-cyan-500/15 hover:bg-cyan-500/25 border border-cyan-500/30 px-2.5 py-1 rounded-lg flex items-center gap-1.5 transition"
                  title="Buka file langsung ({primaryFile})"
                >
                  <ExternalLink class="w-3 h-3" />
                  <span>Open File</span>
                </button>
              {/if}
              {#if folderTarget}
                <button
                  on:click={() => handleOpenFolder(folderTarget)}
                  class="text-[11px] font-medium text-slate-300 hover:text-white bg-slate-800 hover:bg-slate-700 border border-slate-700 px-2.5 py-1 rounded-lg flex items-center gap-1.5 transition"
                  title="Buka folder lokasi file ({folderTarget})"
                >
                  <FolderOpen class="w-3 h-3 text-amber-400" />
                  <span>Open Folder</span>
                </button>
              {/if}
            </div>
          {/if}
        </div>
      {:else}
        {#if $activeTransfers.length === 0}
          <div class="h-44 flex flex-col items-center justify-center text-center px-4">
            <p class="text-xs text-slate-400">No active or historical transfers yet.</p>
            <p class="text-[11px] text-slate-500 mt-1">
              Right-click any file or folder in your OS Explorer/Finder and choose "Send with Raydrop".
            </p>
          </div>
        {/if}
      {/each}
    </div>
  </div>
</div>

