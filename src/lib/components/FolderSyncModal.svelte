<script lang="ts">
  import { onMount } from 'svelte';
  import { open } from '@tauri-apps/plugin-dialog';
  import { contacts } from '../stores/contacts';
  import { raydropApi, type FolderSyncTask } from '../utils/tauri-ipc';
  import {
    FolderSync,
    FolderPlus,
    FolderOpen,
    RefreshCw,
    Trash2,
    Check,
    X,
    Sparkles,
    Laptop,
    Clock,
    Play,
    Zap,
    AlertCircle,
  } from 'lucide-svelte';

  export let onClose: () => void;

  let tasks: FolderSyncTask[] = [];
  let loading = true;
  let syncingTaskId = '';
  let statusBanner = '';
  let statusType: 'success' | 'error' | '' = '';

  // Form states for adding new task
  let showAddForm = false;
  let selectedPath = '';
  let selectedPeerId = '';
  let autoSync = false;
  let isSubmitting = false;

  $: trustedContacts = $contacts.filter(
    (c) => c.is_own_device || c.trust_level === 'trusted_auto_accept'
  );

  onMount(async () => {
    await loadTasks();
  });

  async function loadTasks() {
    loading = true;
    try {
      tasks = await raydropApi.listFolderSyncTasks();
    } catch (err: any) {
      console.warn('Failed to load folder sync tasks:', err);
    } finally {
      loading = false;
    }
  }

  async function handlePickFolder() {
    try {
      const picked = await open({
        directory: true,
        multiple: false,
        title: 'Select Local Folder to Sync',
      });
      if (typeof picked === 'string') {
        selectedPath = picked;
      }
    } catch (err: any) {
      statusBanner = `Folder picker error: ${String(err)}`;
      statusType = 'error';
    }
  }

  async function handleAddTask() {
    if (!selectedPath.trim()) {
      statusBanner = 'Please select a local folder to sync';
      statusType = 'error';
      return;
    }
    if (!selectedPeerId) {
      statusBanner = 'Please select a target destination device';
      statusType = 'error';
      return;
    }

    isSubmitting = true;
    statusBanner = '';
    try {
      const newTask = await raydropApi.addFolderSyncTask({
        localPath: selectedPath,
        targetDeviceId: selectedPeerId,
        isAutoSync: autoSync,
      });
      tasks = [newTask, ...tasks];
      selectedPath = '';
      selectedPeerId = '';
      autoSync = false;
      showAddForm = false;
      statusBanner = 'Folder sync task created successfully!';
      statusType = 'success';
      setTimeout(() => (statusBanner = ''), 3000);
    } catch (err: any) {
      statusBanner = err?.toString() || 'Failed to add sync task';
      statusType = 'error';
    } finally {
      isSubmitting = false;
    }
  }

  async function handleSyncNow(taskId: string) {
    syncingTaskId = taskId;
    statusBanner = '';
    try {
      await raydropApi.triggerFolderSync(taskId);
      statusBanner = 'Folder transfer initiated! Progress stream is active.';
      statusType = 'success';
      await loadTasks();
      setTimeout(() => (statusBanner = ''), 3000);
    } catch (err: any) {
      statusBanner = err?.toString() || 'Failed to sync folder';
      statusType = 'error';
    } finally {
      syncingTaskId = '';
    }
  }

  async function handleToggleAuto(task: FolderSyncTask) {
    try {
      const next = !task.is_auto_sync;
      await raydropApi.toggleFolderSyncAuto(task.id, next);
      tasks = tasks.map((t) => (t.id === task.id ? { ...t, is_auto_sync: next } : t));
    } catch (err: any) {
      statusBanner = `Auto-sync toggle error: ${String(err)}`;
      statusType = 'error';
    }
  }

  async function handleDeleteTask(taskId: string) {
    try {
      await raydropApi.deleteFolderSyncTask(taskId);
      tasks = tasks.filter((t) => t.id !== taskId);
      statusBanner = 'Sync task removed';
      statusType = 'success';
      setTimeout(() => (statusBanner = ''), 2000);
    } catch (err: any) {
      statusBanner = `Delete error: ${String(err)}`;
      statusType = 'error';
    }
  }

  function formatTime(unixSecs?: number | null): string {
    if (!unixSecs) return 'Never';
    const date = new Date(unixSecs * 1000);
    const now = new Date();
    const isToday = date.toDateString() === now.toDateString();
    const hours = String(date.getHours()).padStart(2, '0');
    const minutes = String(date.getMinutes()).padStart(2, '0');
    if (isToday) return `Today, ${hours}:${minutes}`;
    return `${date.toLocaleDateString()}, ${hours}:${minutes}`;
  }

  function getFolderBaseName(fullPath: string): string {
    const parts = fullPath.split(/[/\\]/);
    return parts[parts.length - 1] || parts[parts.length - 2] || fullPath;
  }
</script>

<div
  class="fixed inset-0 z-50 bg-slate-950/80 backdrop-blur-md flex items-center justify-center p-4"
  role="dialog"
  aria-modal="true"
>
  <div class="w-full max-w-xl bg-slate-900 border border-emerald-500/30 rounded-2xl p-6 shadow-2xl flex flex-col gap-4 max-h-[90vh] overflow-hidden">
    <!-- Header -->
    <div class="flex items-center justify-between pb-3 border-b border-slate-800 shrink-0">
      <div class="flex items-center gap-3">
        <div class="w-10 h-10 rounded-xl bg-gradient-to-br from-emerald-500/20 to-teal-500/20 border border-emerald-500/40 flex items-center justify-center text-emerald-400">
          <FolderSync class="w-5 h-5" />
        </div>
        <div>
          <h3 class="text-base font-bold text-white flex items-center gap-2">
            <span>Folder Mirror & Sync</span>
            <span class="text-[10px] font-mono font-bold px-2 py-0.5 rounded-full bg-emerald-500/15 text-emerald-300 border border-emerald-500/30">
              One-Way Mirror
            </span>
          </h3>
          <p class="text-xs text-slate-400 mt-0.5">
            Synchronize entire directory trees directly to your trusted peers with zero cloud storage
          </p>
        </div>
      </div>
      <button
        on:click={onClose}
        class="p-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-400 hover:text-white transition-colors"
        title="Close"
      >
        <X class="w-4 h-4" />
      </button>
    </div>

    <!-- Status Banner -->
    {#if statusBanner}
      <div
        class="p-2.5 rounded-xl text-xs flex items-center justify-between shrink-0 {statusType === 'success'
          ? 'bg-emerald-500/10 border border-emerald-500/30 text-emerald-300'
          : 'bg-rose-500/10 border border-rose-500/30 text-rose-300'}"
      >
        <span>{statusBanner}</span>
        <button on:click={() => (statusBanner = '')} class="text-slate-400 hover:text-white ml-2">✕</button>
      </div>
    {/if}

    <!-- Add Task Form or Button -->
    {#if showAddForm}
      <div class="p-4 rounded-xl bg-slate-950/80 border border-emerald-500/30 space-y-3 shrink-0 animate-in fade-in duration-150">
        <div class="flex items-center justify-between">
          <span class="text-xs font-bold text-white flex items-center gap-1.5">
            <FolderPlus class="w-4 h-4 text-emerald-400" />
            <span>Configure New Folder Mirror</span>
          </span>
          <button
            type="button"
            on:click={() => (showAddForm = false)}
            class="text-xs text-slate-400 hover:text-white"
          >
            Cancel
          </button>
        </div>

        <!-- Folder Picker -->
        <div>
          <span class="block text-[11px] font-semibold text-slate-300 mb-1">Local Directory</span>
          <div class="flex items-center gap-2">
            <input
              type="text"
              readonly
              value={selectedPath}
              placeholder="Click Browse to select folder..."
              class="flex-1 px-3 py-2 rounded-xl bg-slate-900 border border-slate-800 text-xs text-white placeholder-slate-500 truncate"
            />
            <button
              type="button"
              on:click={handlePickFolder}
              class="px-3 py-2 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs font-semibold border border-slate-700 transition shrink-0"
            >
              Browse...
            </button>
          </div>
        </div>

        <!-- Target Peer Picker -->
        <div>
          <label for="sync-peer-select" class="block text-[11px] font-semibold text-slate-300 mb-1">Target Device</label>
          <select
            id="sync-peer-select"
            bind:value={selectedPeerId}
            class="w-full px-3 py-2 rounded-xl bg-slate-900 border border-slate-800 text-xs text-white focus:outline-none focus:border-emerald-500"
          >
            <option value="" disabled selected>Select destination device...</option>
            {#each $contacts as contact}
              <option value={contact.device_id}>
                {contact.is_online ? '🟢' : '⚪'} {contact.alias_name} ({contact.is_own_device ? 'My Device' : contact.trust_level})
              </option>
            {/each}
          </select>
        </div>

        <!-- Auto Sync Toggle -->
        <label class="flex items-center gap-2.5 text-xs text-slate-300 cursor-pointer pt-1">
          <input
            type="checkbox"
            bind:checked={autoSync}
            class="rounded bg-slate-900 border-slate-700 text-emerald-500 focus:ring-emerald-500/30"
          />
          <span>Auto-sync changes (transfers file tree on demand)</span>
        </label>

        <!-- Submit Button -->
        <div class="flex justify-end gap-2 pt-2">
          <button
            type="button"
            on:click={handleAddTask}
            disabled={isSubmitting || !selectedPath || !selectedPeerId}
            class="flex items-center gap-1.5 px-4 py-2 rounded-xl bg-gradient-to-r from-emerald-500 to-teal-600 hover:from-emerald-400 hover:to-teal-500 text-slate-950 font-bold text-xs shadow-md shadow-emerald-500/20 transition disabled:opacity-50"
          >
            <Check class="w-3.5 h-3.5 stroke-[2.5]" />
            <span>Create Mirror Task</span>
          </button>
        </div>
      </div>
    {:else}
      <button
        type="button"
        on:click={() => (showAddForm = true)}
        class="p-3 rounded-xl border border-dashed border-emerald-500/40 hover:border-emerald-400 bg-emerald-500/5 hover:bg-emerald-500/10 text-emerald-300 text-xs font-semibold flex items-center justify-center gap-2 transition shrink-0"
      >
        <FolderPlus class="w-4 h-4 text-emerald-400" />
        <span>+ Add New Folder to Sync</span>
      </button>
    {/if}

    <!-- Tasks List -->
    <div class="flex-1 overflow-y-auto space-y-2.5 pr-1">
      {#if loading}
        <div class="p-8 text-center text-xs text-slate-400">Loading sync tasks...</div>
      {:else if tasks.length === 0}
        <div class="p-8 rounded-xl border border-slate-800 text-center space-y-2 bg-slate-950/40">
          <FolderSync class="w-8 h-8 text-slate-600 mx-auto" />
          <h4 class="text-xs font-bold text-white">No Folder Sync Pairs Configured</h4>
          <p class="text-[11px] text-slate-400 max-w-sm mx-auto">
            Mirror your workspace, code repo, or camera photos to your desktop or laptop effortlessly with 1-click sync.
          </p>
        </div>
      {:else}
        {#each tasks as task (task.id)}
          <div class="p-3.5 rounded-xl bg-slate-950/80 border border-slate-800/90 hover:border-slate-700 flex flex-col sm:flex-row sm:items-center justify-between gap-3 transition">
            <div class="min-w-0 flex items-start gap-3">
              <div class="w-9 h-9 rounded-xl bg-emerald-500/15 border border-emerald-500/30 flex items-center justify-center text-emerald-400 shrink-0 mt-0.5">
                <FolderOpen class="w-4 h-4" />
              </div>
              <div class="min-w-0">
                <div class="flex items-center gap-2">
                  <h4 class="text-xs font-bold text-white truncate" title={task.local_path}>
                    {getFolderBaseName(task.local_path)}
                  </h4>
                  <span class="text-[10px] font-mono px-1.5 py-0.2 rounded bg-slate-800 text-slate-400 border border-slate-700">
                    &rarr; {task.target_alias}
                  </span>
                </div>
                <p class="text-[11px] font-mono text-slate-400 truncate mt-0.5" title={task.local_path}>
                  {task.local_path}
                </p>
                <div class="flex items-center gap-2 mt-1 text-[10px] font-mono text-slate-500">
                  <span class="flex items-center gap-1">
                    <Clock class="w-3 h-3 text-slate-500" />
                    Last sync: {formatTime(task.last_sync_at)}
                  </span>
                </div>
              </div>
            </div>

            <!-- Actions: Sync Now & Delete -->
            <div class="flex items-center gap-2 shrink-0 self-end sm:self-center">
              <button
                type="button"
                on:click={() => handleToggleAuto(task)}
                class="px-2 py-1 rounded-lg text-[10px] font-mono font-semibold transition border {task.is_auto_sync
                  ? 'bg-emerald-500/20 text-emerald-300 border-emerald-500/40'
                  : 'bg-slate-800 text-slate-400 border-slate-700'}"
                title="Toggle Auto-sync flag"
              >
                {task.is_auto_sync ? 'Auto: ON' : 'Auto: OFF'}
              </button>

              <button
                type="button"
                on:click={() => handleSyncNow(task.id)}
                disabled={syncingTaskId === task.id}
                class="flex items-center gap-1.5 px-3 py-1.5 rounded-xl bg-gradient-to-r from-emerald-500 to-teal-600 hover:from-emerald-400 hover:to-teal-500 text-slate-950 font-bold text-xs shadow-md shadow-emerald-500/20 transition disabled:opacity-50"
                title="Push all files in this folder to destination"
              >
                <RefreshCw class="w-3 h-3 {syncingTaskId === task.id ? 'animate-spin' : ''}" />
                <span>{syncingTaskId === task.id ? 'Syncing...' : 'Sync Now'}</span>
              </button>

              <button
                type="button"
                on:click={() => handleDeleteTask(task.id)}
                class="p-1.5 text-slate-500 hover:text-rose-400 hover:bg-rose-500/10 rounded-lg transition"
                title="Delete this sync task"
              >
                <Trash2 class="w-3.5 h-3.5" />
              </button>
            </div>
          </div>
        {/each}
      {/if}
    </div>

    <!-- Footer -->
    <div class="flex items-center justify-between pt-3 border-t border-slate-800 text-xs shrink-0">
      <span class="text-slate-500 text-[11px] font-mono">Auto-accept trusted peers eliminates confirmation prompts</span>
      <button
        type="button"
        on:click={onClose}
        class="px-4 py-1.5 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-300 font-semibold text-xs transition"
      >
        Close
      </button>
    </div>
  </div>
</div>
