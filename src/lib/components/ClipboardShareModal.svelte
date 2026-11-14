<script lang="ts">
  import { onMount } from 'svelte';
  import { contacts, lanContacts } from '../stores/contacts';
  import { raydropApi } from '../utils/tauri-ipc';
  import {
    Clipboard,
    ClipboardCopy,
    Send,
    Check,
    X,
    Sparkles,
    Smartphone,
    Laptop,
    Globe,
    Radio,
    ShieldCheck,
    RefreshCw,
    ExternalLink,
  } from 'lucide-svelte';

  export let onClose: () => void;

  let textContent = '';
  let selectedTarget: 'all_trusted' | string = 'all_trusted';
  let isSending = false;
  let statusMessage = '';
  let statusType: 'success' | 'error' | '' = '';
  let readingClipboard = false;

  $: isUrl = /^https?:\/\/\S+/i.test(textContent.trim());

  $: trustedContacts = $contacts.filter(
    (c) => c.is_own_device || c.trust_level === 'trusted_auto_accept'
  );

  onMount(async () => {
    await fetchLocalClipboard();
  });

  async function fetchLocalClipboard() {
    readingClipboard = true;
    try {
      const text = await raydropApi.readSystemClipboard();
      if (text && text.trim().length > 0) {
        textContent = text;
      }
    } catch (err) {
      console.warn('Could not read OS clipboard automatically:', err);
    } finally {
      readingClipboard = false;
    }
  }

  async function handleSend() {
    if (!textContent.trim()) {
      statusMessage = 'Clipboard text cannot be empty';
      statusType = 'error';
      return;
    }

    isSending = true;
    statusMessage = '';
    statusType = '';

    try {
      if (selectedTarget === 'all_trusted') {
        const count = await raydropApi.broadcastClipboard(textContent);
        statusMessage = `Successfully synced to ${count} trusted device(s)!`;
        statusType = 'success';
        setTimeout(() => {
          onClose();
        }, 1200);
      } else {
        await raydropApi.sendChatMessage(
          selectedTarget,
          textContent,
          'clipboard'
        );
        statusMessage = 'Clipboard sent to selected device!';
        statusType = 'success';
        setTimeout(() => {
          onClose();
        }, 1200);
      }
    } catch (err: any) {
      statusMessage = err?.toString() || 'Failed to send clipboard';
      statusType = 'error';
    } finally {
      isSending = false;
    }
  }
</script>

<div
  class="fixed inset-0 z-50 bg-slate-950/80 backdrop-blur-md flex items-center justify-center p-4"
  role="dialog"
  aria-modal="true"
>
  <div class="w-full max-w-lg bg-slate-900 border border-indigo-500/30 rounded-2xl p-6 shadow-2xl flex flex-col gap-4 max-h-[90vh] overflow-y-auto">
    <!-- Header -->
    <div class="flex items-center justify-between pb-3 border-b border-slate-800">
      <div class="flex items-center gap-3">
        <div class="w-10 h-10 rounded-xl bg-gradient-to-br from-indigo-500/20 to-purple-500/20 border border-indigo-500/40 flex items-center justify-center text-indigo-400">
          <Clipboard class="w-5 h-5" />
        </div>
        <div>
          <h3 class="text-base font-bold text-white flex items-center gap-2">
            <span>Universal Clipboard</span>
            <span class="text-[10px] font-mono font-bold px-2 py-0.5 rounded-full bg-indigo-500/15 text-indigo-300 border border-indigo-500/30">
              P2P Instant Sync
            </span>
          </h3>
          <p class="text-xs text-slate-400 mt-0.5">
            Send text, links, or code directly to your other devices' clipboard
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

    <!-- Text Area & Actions -->
    <div class="space-y-2">
      <div class="flex items-center justify-between text-xs">
        <div class="flex items-center gap-2">
          <span class="font-semibold text-slate-300">Clipboard Content</span>
          {#if isUrl}
            <span class="px-2 py-0.5 rounded-full bg-cyan-500/15 text-cyan-300 border border-cyan-500/30 text-[10px] font-mono flex items-center gap-1">
              <Globe class="w-3 h-3" />
              URL Link
            </span>
          {/if}
        </div>
        <button
          type="button"
          on:click={fetchLocalClipboard}
          disabled={readingClipboard}
          class="flex items-center gap-1 text-[11px] text-indigo-400 hover:text-indigo-300 transition-colors"
          title="Paste latest text from OS clipboard"
        >
          <RefreshCw class="w-3 h-3 {readingClipboard ? 'animate-spin' : ''}" />
          <span>Paste from OS</span>
        </button>
      </div>

      <div class="relative">
        <textarea
          bind:value={textContent}
          rows="5"
          placeholder="Paste or type text, link, snippet to sync..."
          class="w-full px-3.5 py-2.5 rounded-xl bg-slate-950 border border-slate-800 focus:border-indigo-500 focus:ring-1 focus:ring-indigo-500/40 text-xs text-white placeholder-slate-500 font-mono resize-none transition"
        ></textarea>
        <div class="absolute bottom-2.5 right-3 text-[10px] font-mono text-slate-500">
          {textContent.length} chars
        </div>
      </div>
    </div>

    <!-- Target Device Selector -->
    <div class="space-y-2">
      <span class="block text-xs font-semibold text-slate-300">Destination</span>
      <div class="grid grid-cols-1 sm:grid-cols-2 gap-2">
        <button
          type="button"
          on:click={() => (selectedTarget = 'all_trusted')}
          class="p-3 rounded-xl border text-left flex items-start gap-2.5 transition-all {selectedTarget === 'all_trusted'
            ? 'bg-indigo-600/20 border-indigo-500/60 shadow-lg shadow-indigo-500/10'
            : 'bg-slate-950/60 border-slate-800 hover:border-slate-700'}"
        >
          <div class="w-7 h-7 rounded-lg bg-indigo-500/20 text-indigo-300 flex items-center justify-center shrink-0 mt-0.5">
            <Sparkles class="w-4 h-4" />
          </div>
          <div class="min-w-0">
            <div class="text-xs font-bold text-white flex items-center gap-1.5">
              <span>All My Devices</span>
              <span class="text-[10px] px-1.5 py-0.2 rounded bg-indigo-500/20 text-indigo-300 font-mono">
                {trustedContacts.length}
              </span>
            </div>
            <p class="text-[10px] text-slate-400 mt-0.5 leading-tight">
              Auto-copies directly to OS clipboard on all trusted devices
            </p>
          </div>
        </button>

        {#if $contacts.length > 0}
          <div class="p-2.5 rounded-xl bg-slate-950/60 border border-slate-800 flex flex-col justify-center gap-1">
            <label for="peer-select" class="text-[10px] font-semibold text-slate-400">Or choose specific peer:</label>
            <select
              id="peer-select"
              bind:value={selectedTarget}
              class="w-full px-2 py-1.5 rounded-lg bg-slate-900 border border-slate-700 text-xs text-white focus:outline-none focus:border-indigo-500"
            >
              <option value="all_trusted">✨ All My Devices ({trustedContacts.length})</option>
              {#each $contacts as contact}
                <option value={contact.device_id}>
                  {contact.is_online ? '🟢' : '⚪'} {contact.alias_name} ({contact.trust_level === 'trusted_auto_accept' ? 'Trusted' : 'Peer'})
                </option>
              {/each}
            </select>
          </div>
        {:else}
          <div class="p-3 rounded-xl bg-slate-950/40 border border-slate-800/80 flex items-center text-[11px] text-slate-500 italic">
            Add contacts to target specific individual peers.
          </div>
        {/if}
      </div>
    </div>

    <!-- Status feedback -->
    {#if statusMessage}
      <div
        class="p-2.5 rounded-xl text-xs flex items-center justify-between {statusType === 'success'
          ? 'bg-emerald-500/10 border border-emerald-500/30 text-emerald-300'
          : 'bg-rose-500/10 border border-rose-500/30 text-rose-300'}"
      >
        <span>{statusMessage}</span>
        {#if statusType === 'success'}
          <Check class="w-4 h-4 text-emerald-400" />
        {/if}
      </div>
    {/if}

    <!-- Action Buttons -->
    <div class="flex items-center justify-end gap-3 pt-2 border-t border-slate-800">
      <button
        type="button"
        on:click={onClose}
        class="px-4 py-2 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-300 text-xs font-semibold transition"
      >
        Cancel
      </button>

      <button
        type="button"
        on:click={handleSend}
        disabled={isSending || !textContent.trim()}
        class="flex items-center gap-2 px-5 py-2 rounded-xl bg-gradient-to-r from-indigo-500 to-purple-600 hover:from-indigo-400 hover:to-purple-500 text-white font-bold text-xs shadow-lg shadow-indigo-500/25 transition disabled:opacity-50 disabled:cursor-not-allowed"
      >
        <Send class="w-3.5 h-3.5" />
        <span>{isSending ? 'Sending...' : 'Send to Clipboard'}</span>
      </button>
    </div>
  </div>
</div>
