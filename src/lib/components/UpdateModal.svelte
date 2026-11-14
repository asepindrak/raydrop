<script lang="ts">
  import {
    updateInfo,
    updateModalOpen,
    updateDownloading,
    updateProgress,
    updateStatusMessage,
    updateErrorMessage,
  } from '../stores/update';
  import { raydropApi, formatBytes } from '../utils/tauri-ipc';
  import { renderMarkdown } from '../utils/markdown';
  import { open as openShell } from '@tauri-apps/plugin-shell';
  import {
    Sparkles,
    Download,
    ArrowUpCircle,
    X,
    ExternalLink,
    AlertCircle,
    CheckCircle2,
    Loader2,
    Calendar,
    HardDrive,
  } from 'lucide-svelte';

  $: isAndroid = /Android/i.test(navigator.userAgent) || ($updateInfo?.asset_name?.toLowerCase().endsWith('.apk') ?? false);

  async function handleUpdateNow() {
    if (!$updateInfo || !$updateInfo.download_url || !$updateInfo.asset_name) {
      if ($updateInfo?.html_url) {
        handleOpenBrowser();
      }
      return;
    }

    updateErrorMessage.set('');
    updateDownloading.set(true);
    updateStatusMessage.set(isAndroid ? 'Mengunduh file APK pembaruan...' : 'Connecting to GitHub Release server...');

    try {
      const result = await raydropApi.downloadAndInstallUpdate(
        $updateInfo.download_url,
        $updateInfo.asset_name
      );
      updateStatusMessage.set(result);
    } catch (err: any) {
      console.error('Update failed:', err);
      updateErrorMessage.set(typeof err === 'string' ? err : err?.message || 'Download failed');
      updateDownloading.set(false);
    }
  }

  async function handleOpenBrowser() {
    if (!$updateInfo?.html_url) return;
    try {
      await openShell($updateInfo.html_url);
    } catch {
      window.open($updateInfo.html_url, '_blank');
    }
  }

  async function handleContentClick(e: MouseEvent) {
    const target = (e.target as HTMLElement)?.closest('a');
    if (!target) return;
    const href = target.getAttribute('href');
    if (href && (href.startsWith('http://') || href.startsWith('https://'))) {
      e.preventDefault();
      try {
        await raydropApi.openExternalUrl(href);
      } catch {
        window.open(href, '_blank');
      }
    }
  }

  function handleClose() {
    if ($updateDownloading) return; // Prevent closing while actively downloading installer
    updateModalOpen.set(false);
  }

  function formatDate(isoStr: string): string {
    if (!isoStr) return '';
    try {
      const d = new Date(isoStr);
      return d.toLocaleDateString(undefined, {
        year: 'numeric',
        month: 'short',
        day: 'numeric',
      });
    } catch {
      return isoStr;
    }
  }
</script>

{#if $updateModalOpen && $updateInfo}
  <div
    class="fixed inset-0 z-50 bg-slate-950/80 backdrop-blur-md flex items-center justify-center p-4 select-none animate-in fade-in duration-200"
  >
    <div
      class="w-full max-w-lg bg-slate-900 border border-cyan-500/30 rounded-2xl p-6 shadow-2xl shadow-cyan-500/10 flex flex-col max-h-[90vh] overflow-hidden"
    >
      <!-- Header -->
      <div class="flex items-center justify-between pb-4 border-b border-slate-800 shrink-0">
        <div class="flex items-center gap-3">
          <div class="w-10 h-10 rounded-xl bg-cyan-500/10 border border-cyan-500/30 flex items-center justify-center text-cyan-400">
            <Sparkles class="w-5 h-5 animate-pulse" />
          </div>
          <div>
            <div class="flex items-center gap-2">
              <h3 class="text-base font-bold text-white tracking-tight">New Update Available</h3>
              <span class="text-[10px] font-mono font-bold px-2 py-0.5 rounded-full bg-cyan-500/20 text-cyan-300 border border-cyan-500/40">
                {$updateInfo.latest_version}
              </span>
            </div>
            <p class="text-xs text-slate-400 flex items-center gap-1.5 mt-0.5">
              <span>Current:</span>
              <span class="font-mono text-slate-300">v{$updateInfo.current_version}</span>
              <span>➜</span>
              <span class="font-mono text-cyan-400 font-semibold">{$updateInfo.latest_version}</span>
            </p>
          </div>
        </div>

        {#if !$updateDownloading}
          <button
            on:click={handleClose}
            class="p-1.5 rounded-lg text-slate-400 hover:text-white hover:bg-slate-800 transition-colors"
            title="Remind me later"
          >
            <X class="w-4 h-4" />
          </button>
        {/if}
      </div>

      <!-- Content -->
      <div class="py-4 flex-1 overflow-y-auto space-y-4 pr-1 text-slate-300">
        <!-- Release Card -->
        <div class="p-3.5 rounded-xl bg-slate-950/70 border border-slate-800/80 space-y-2">
          <div class="flex items-center justify-between text-xs">
            <span class="font-semibold text-white">
              {$updateInfo.release_title || $updateInfo.latest_version}
            </span>
            {#if $updateInfo.release_date}
              <span class="text-[11px] text-slate-400 flex items-center gap-1 font-mono">
                <Calendar class="w-3 h-3 text-cyan-400" />
                {formatDate($updateInfo.release_date)}
              </span>
            {/if}
          </div>

          {#if $updateInfo.asset_name}
            <div class="flex items-center gap-2 text-[11px] text-slate-400 font-mono">
              <HardDrive class="w-3 h-3 text-slate-500" />
              <span>{$updateInfo.asset_name}</span>
              {#if $updateInfo.asset_size}
                <span class="text-slate-500">•</span>
                <span>{formatBytes($updateInfo.asset_size)}</span>
              {/if}
            </div>
          {/if}
        </div>

        <!-- Release Notes -->
        {#if $updateInfo.release_notes}
          <div>
            <h4 class="text-xs font-semibold text-slate-300 mb-1.5 flex items-center gap-1.5">
              <span>Release Notes</span>
            </h4>
            <!-- svelte-ignore a11y-click-events-have-key-events -->
            <!-- svelte-ignore a11y-no-static-element-interactions -->
            <div
              on:click={handleContentClick}
              class="p-3.5 rounded-xl bg-slate-950 border border-slate-800/70 text-xs text-slate-300 leading-relaxed max-h-56 overflow-y-auto select-text markdown-body"
            >
              {@html renderMarkdown($updateInfo.release_notes)}
            </div>
          </div>
        {/if}

        <!-- Download Progress Bar -->
        {#if $updateDownloading}
          <div class="p-3.5 rounded-xl bg-cyan-950/20 border border-cyan-500/30 space-y-2.5">
            <div class="flex items-center justify-between text-xs">
              <span class="text-cyan-300 font-medium flex items-center gap-1.5">
                <Loader2 class="w-3.5 h-3.5 animate-spin text-cyan-400" />
                {$updateStatusMessage || 'Downloading update...'}
              </span>
              <span class="font-mono text-cyan-400 font-bold">
                {($updateProgress?.percent || 0).toFixed(1)}%
              </span>
            </div>

            <!-- Bar -->
            <div class="w-full bg-slate-800 rounded-full h-2 overflow-hidden">
              <div
                class="bg-gradient-to-r from-cyan-500 to-blue-500 h-2 rounded-full transition-all duration-150"
                style="width: {$updateProgress?.percent || 0}%"
              ></div>
            </div>

            <!-- Stats -->
            <div class="flex items-center justify-between text-[11px] text-slate-400 font-mono">
              <span>
                {formatBytes($updateProgress?.downloaded || 0)} / {formatBytes($updateProgress?.total || $updateInfo.asset_size || 0)}
              </span>
              {#if $updateProgress?.speed_bytes_sec}
                <span class="text-cyan-400">
                  {formatBytes($updateProgress.speed_bytes_sec)}/s
                </span>
              {/if}
            </div>
          </div>
        {/if}

        <!-- Error Message -->
        {#if $updateErrorMessage}
          <div class="p-3 rounded-xl bg-red-950/30 border border-red-500/30 flex items-center gap-2.5 text-xs text-red-300">
            <AlertCircle class="w-4 h-4 text-red-400 shrink-0" />
            <span class="flex-1">{$updateErrorMessage}</span>
          </div>
        {/if}
      </div>

      <!-- Actions -->
      <div class="pt-4 border-t border-slate-800 flex items-center justify-between gap-3 shrink-0">
        <button
          type="button"
          on:click={handleOpenBrowser}
          class="flex items-center gap-1.5 px-3 py-2 rounded-xl bg-slate-800/80 hover:bg-slate-800 border border-slate-700 text-xs font-medium text-slate-300 hover:text-white transition-colors"
        >
          <ExternalLink class="w-3.5 h-3.5" />
          <span>GitHub Releases</span>
        </button>

        <div class="flex items-center gap-2">
          {#if !$updateDownloading}
            <button
              type="button"
              on:click={handleClose}
              class="px-3.5 py-2 rounded-xl bg-slate-800 hover:bg-slate-700 text-xs font-semibold text-slate-400 hover:text-white transition-colors"
            >
              Remind Later
            </button>
            <button
              type="button"
              on:click={handleUpdateNow}
              class="flex items-center gap-1.5 px-4 py-2 rounded-xl bg-gradient-to-r from-cyan-500 to-blue-600 hover:from-cyan-400 hover:to-blue-500 text-xs font-bold text-white shadow-lg shadow-cyan-500/20 active:scale-95 transition-all"
            >
              <Download class="w-3.5 h-3.5" />
              <span>{isAndroid ? 'Download APK' : 'Update Now'}</span>
            </button>
          {:else}
            <button
              disabled
              class="flex items-center gap-1.5 px-4 py-2 rounded-xl bg-slate-800 border border-slate-700 text-xs font-semibold text-slate-400 cursor-not-allowed"
            >
              <Loader2 class="w-3.5 h-3.5 animate-spin text-cyan-400" />
              <span>{isAndroid ? 'Downloading APK...' : 'Installing...'}</span>
            </button>
          {/if}
        </div>
      </div>
    </div>
  </div>
{/if}

<style>
  :global(.markdown-body a) {
    color: #38bdf8 !important;
    text-decoration: underline !important;
    cursor: pointer !important;
  }
  :global(.markdown-body a:hover) {
    color: #7dd3fc !important;
  }
  :global(.markdown-body h1) {
    font-size: 1.05rem;
    font-weight: 700;
    color: #f1f5f9;
    margin: 0.75rem 0 0.35rem 0;
  }
  :global(.markdown-body h2) {
    font-size: 0.95rem;
    font-weight: 700;
    color: #e2e8f0;
    margin: 0.6rem 0 0.3rem 0;
  }
  :global(.markdown-body h3) {
    font-size: 0.85rem;
    font-weight: 600;
    color: #cbd5e1;
    margin: 0.5rem 0 0.25rem 0;
  }
  :global(.markdown-body h4) {
    font-size: 0.8rem;
    font-weight: 600;
    color: #94a3b8;
    margin: 0.4rem 0 0.2rem 0;
  }
  :global(.markdown-body pre) {
    background-color: #020617 !important;
    border: 1px solid #1e293b;
    border-radius: 0.5rem;
    padding: 0.5rem 0.75rem;
    overflow-x: auto;
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 0.75rem;
    margin: 0.4rem 0;
  }
  :global(.markdown-body code) {
    background-color: rgba(15, 23, 42, 0.8);
    border: 1px solid rgba(51, 65, 85, 0.6);
    border-radius: 0.25rem;
    padding: 0.1rem 0.3rem;
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 0.75rem;
    color: #38bdf8;
  }
  :global(.markdown-body p) {
    margin-bottom: 0.4rem;
  }
  :global(.markdown-body p:last-child) {
    margin-bottom: 0;
  }
  :global(.markdown-body ul) {
    list-style-type: disc;
    padding-left: 1.25rem;
    margin: 0.35rem 0;
  }
  :global(.markdown-body ol) {
    list-style-type: decimal;
    padding-left: 1.25rem;
    margin: 0.35rem 0;
  }
  :global(.markdown-body li) {
    margin: 0.15rem 0;
  }
  :global(.markdown-body blockquote) {
    border-left: 3px solid #06b6d4;
    padding-left: 0.75rem;
    color: #94a3b8;
    margin: 0.35rem 0;
  }
  :global(.markdown-body hr) {
    border: none;
    border-top: 1px solid #334155;
    margin: 0.6rem 0;
  }
  :global(.markdown-body table) {
    border-collapse: collapse;
    width: 100%;
    margin: 0.5rem 0;
  }
  :global(.markdown-body th, .markdown-body td) {
    border: 1px solid #334155;
    padding: 0.3rem 0.6rem;
    text-align: left;
  }
  :global(.markdown-body th) {
    background-color: rgba(15, 23, 42, 0.8);
  }
</style>
