<script lang="ts">
  import { activeTransfers, historicalTransfers } from '../stores/transfers';
  import { raydropApi, formatBytes } from '../utils/tauri-ipc';
  import {
    ArrowUpRight,
    ArrowDownLeft,
    XCircle,
    CheckCircle2,
    AlertTriangle,
    Activity,
    ShieldCheck,
  } from 'lucide-svelte';

  function calcPercent(transferred: number, total: number): number {
    if (total <= 0) return 0;
    return Math.min(100, Math.round((transferred / total) * 100));
  }
</script>

<div class="flex flex-col h-full bg-slate-900/40 border border-slate-800/80 rounded-2xl overflow-hidden">
  <div class="p-4 border-b border-slate-800/80 flex items-center justify-between">
    <div class="flex items-center gap-2">
      <Activity class="w-4 h-4 text-cyan-400" />
      <h2 class="text-sm font-bold text-white">Transfers & Encrypted Streams</h2>
    </div>
    <div class="flex items-center gap-1.5 text-[11px] text-emerald-400 bg-emerald-500/10 px-2.5 py-1 rounded-full border border-emerald-500/20">
      <ShieldCheck class="w-3.5 h-3.5" />
      <span>ChaCha20-Poly1305 + BLAKE3</span>
    </div>
  </div>

  <div class="flex-1 overflow-y-auto p-4 space-y-3">
    <!-- Active Streaming Transfers -->
    {#if $activeTransfers.length > 0}
      <div class="space-y-2.5">
        <p class="text-[11px] font-semibold uppercase tracking-wider text-cyan-400">
          Active Streams ({$activeTransfers.length})
        </p>
        {#each $activeTransfers as t (t.id)}
          {@const pct = calcPercent(t.transferred_bytes, t.total_bytes)}
          <div class="p-3.5 rounded-xl bg-slate-900 border border-cyan-500/30 space-y-2.5">
            <div class="flex items-center justify-between">
              <div class="flex items-center gap-2">
                {#if t.direction === 'outgoing'}
                  <ArrowUpRight class="w-4 h-4 text-cyan-400" />
                {:else}
                  <ArrowDownLeft class="w-4 h-4 text-emerald-400" />
                {/if}
                <span class="text-xs font-semibold text-white">{t.peer_alias}</span>
                <span class="text-[10px] font-mono uppercase px-1.5 py-0.5 rounded bg-slate-800 text-cyan-300 border border-slate-700">
                  {t.network_route === 'lan_direct' ? 'LAN' : 'WAN P2P'}
                </span>
              </div>
              <button
                on:click={() => raydropApi.cancelTransfer(t.id)}
                class="text-xs text-rose-400 hover:text-rose-300 flex items-center gap-1"
              >
                <XCircle class="w-3.5 h-3.5" />
                <span>Cancel</span>
              </button>
            </div>

            <!-- Progress Bar -->
            <div class="w-full h-2 rounded-full bg-slate-950 overflow-hidden border border-slate-800">
              <div
                class="h-full bg-gradient-to-r from-cyan-400 to-blue-500 transition-all duration-150"
                style="width: {pct}%"
              ></div>
            </div>

            <div class="flex items-center justify-between text-[11px] font-mono text-slate-400">
              <span>{formatBytes(t.transferred_bytes)} / {formatBytes(t.total_bytes)} ({pct}%)</span>
              <span>
                {(t.speed_mbps ?? 0).toFixed(1)} MB/s • ETA {t.eta_seconds ?? 0}s
              </span>
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
        <div class="p-3 rounded-xl bg-slate-900/70 border border-slate-800/80 flex items-center justify-between gap-2">
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
              <p class="text-[11px] font-mono text-slate-400">
                {h.total_files} file(s) • {formatBytes(h.total_bytes)}
              </p>
            </div>
          </div>
          <span
            class="text-[10px] font-semibold uppercase px-2 py-0.5 rounded-full {h.status ===
            'completed'
              ? 'bg-emerald-500/10 text-emerald-300 border border-emerald-500/20'
              : 'bg-rose-500/10 text-rose-300 border border-rose-500/20'}"
          >
            {h.status}
          </span>
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

