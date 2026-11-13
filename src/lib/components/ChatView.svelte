<script lang="ts">
  import { onMount, tick, onDestroy } from 'svelte';
  import { open } from '@tauri-apps/plugin-dialog';
  import {
    Send,
    Paperclip,
    FolderUp,
    Trash2,
    Check,
    CheckCheck,
    Clock,
    FileText,
    FolderOpen,
    ExternalLink,
    Search,
    MessageSquare,
    Wifi,
    Globe,
    Laptop,
    Sparkles,
    Code,
    CheckCircle2,
    RefreshCw,
    X,
    User,
    ArrowUpRight,
    ArrowDownLeft
  } from 'lucide-svelte';
  import { contacts, lanContacts } from '../stores/contacts';
  import { cloudContacts, cloudMyDevices, activeChatPeerId } from '../stores/state';
  import {
    chatStore,
    unreadCounts,
    loadChatHistory,
    markChatRead,
    clearChatHistory,
    handleOutgoingChatMessage
  } from '../stores/chat';
  import { transfers } from '../stores/transfers';
  import {
    raydropApi,
    formatBytes,
    type ChatMessage,
    type ChatFileItem,
    type TransferRecord
  } from '../utils/tauri-ipc';
  import { renderMarkdown } from '../utils/markdown';

  let messageInput = '';
  let searchQuery = '';
  let chatScrollContainer: HTMLDivElement;
  let isSending = false;
  let copiedMessageId = '';
  let attachingFiles: string[] = [];

  $: allPeers = [
    ...$contacts.map((c) => ({
      id: c.device_id,
      name: c.alias_name || c.user_name || 'Raydrop Peer',
      email: c.user_email || '',
      osType: c.os_type || 'windows',
      isOnline: c.is_online,
      networkType: c.network_type || 'wan',
      isOwnDevice: c.is_own_device,
    })),
  ].filter((p, index, self) => index === self.findIndex((t) => t.id === p.id));

  $: filteredPeers = allPeers.filter((p) => {
    if (!searchQuery.trim()) return true;
    const q = searchQuery.toLowerCase();
    return p.name.toLowerCase().includes(q) || p.email.toLowerCase().includes(q);
  });

  $: activePeer = allPeers.find((p) => p.id === $activeChatPeerId) || null;
  $: activeMessages = ($activeChatPeerId ? $chatStore[$activeChatPeerId] : []) || [];

  // Auto-select first peer if none selected
  $: if (!$activeChatPeerId && filteredPeers.length > 0) {
    activeChatPeerId.set(filteredPeers[0].id);
  }

  // When active peer changes, load chat history and scroll to bottom
  let prevPeerId: string | null = null;
  $: if ($activeChatPeerId && $activeChatPeerId !== prevPeerId) {
    prevPeerId = $activeChatPeerId;
    loadChatHistory($activeChatPeerId).then(() => {
      scrollToBottom();
    });
  }

  // Scroll to bottom when new messages arrive
  $: if (activeMessages.length) {
    tick().then(() => scrollToBottom());
  }

  function scrollToBottom() {
    if (chatScrollContainer) {
      chatScrollContainer.scrollTop = chatScrollContainer.scrollHeight;
    }
  }

  function getBaseName(path: string): string {
    const parts = path.split(/[/\\]/);
    return parts[parts.length - 1] || path;
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
    if (['RS', 'TS', 'JS', 'PY', 'GO', 'CPP', 'C', 'HTML', 'CSS', 'JSON', 'SH'].includes(ext)) {
      return { label: ext, color: 'bg-cyan-500/20 text-cyan-300 border-cyan-500/40' };
    }
    return { label: ext.slice(0, 4), color: 'bg-slate-800 text-slate-300 border-slate-700' };
  }

  function parseFilesJson(jsonStr?: string | null): ChatFileItem[] {
    if (!jsonStr) return [];
    try {
      const parsed = JSON.parse(jsonStr);
      if (Array.isArray(parsed)) return parsed;
    } catch {
      // ignore
    }
    return [];
  }

  function getLinkedTransfer(sessionId?: string | null): TransferRecord | undefined {
    if (!sessionId) return undefined;
    return $transfers.find((t) => t.id === sessionId);
  }

  async function handlePickFiles() {
    try {
      const picked = await open({
        multiple: true,
        title: 'Select Files to Send in Chat',
      });
      if (picked) {
        const list = Array.isArray(picked) ? picked : [picked];
        attachingFiles = [...attachingFiles, ...list];
      }
    } catch (err) {
      console.warn('File selection error:', err);
    }
  }

  async function handlePickFolder() {
    try {
      const picked = await open({
        directory: true,
        multiple: false,
        title: 'Select Folder to Send in Chat',
      });
      if (typeof picked === 'string') {
        attachingFiles = [...attachingFiles, picked];
      }
    } catch (err) {
      console.warn('Folder selection error:', err);
    }
  }

  function removeAttachedFile(index: number) {
    attachingFiles = attachingFiles.filter((_, i) => i !== index);
  }

  async function handleSendMessage() {
    if (!$activeChatPeerId) return;
    const text = messageInput.trim();
    if (!text && attachingFiles.length === 0) return;

    isSending = true;
    try {
      let transferSessionId: string | null = null;
      let filesJson: string | null = null;
      let msgType = 'text';

      if (attachingFiles.length > 0) {
        msgType = 'file_transfer';
        // 1. Initiate accelerated P2P transfer session in Raydrop backend
        transferSessionId = await raydropApi.initiateTransfer(
          $activeChatPeerId,
          attachingFiles
        );

        const filesMeta: ChatFileItem[] = attachingFiles.map((fp) => ({
          name: getBaseName(fp),
          size: 0,
          path: fp,
        }));
        filesJson = JSON.stringify(filesMeta);
      }

      const sentMsg = await raydropApi.sendChatMessage(
        $activeChatPeerId,
        text,
        msgType,
        transferSessionId,
        filesJson
      );

      handleOutgoingChatMessage(sentMsg);
      messageInput = '';
      attachingFiles = [];
      await tick();
      scrollToBottom();
    } catch (err) {
      console.error('Failed to send message:', err);
    } finally {
      isSending = false;
    }
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSendMessage();
    }
  }

  async function handleOpenFile(path: string) {
    try {
      await raydropApi.openFilePath(path);
    } catch (err) {
      console.warn('Failed to open file:', err);
    }
  }

  async function handleOpenFolder(path: string) {
    try {
      await raydropApi.openFolderPath(path);
    } catch (err) {
      console.warn('Failed to open folder:', err);
    }
  }

  async function handleClearHistory() {
    if (!$activeChatPeerId) return;
    if (confirm('Clear chat history with this peer on your device?')) {
      await clearChatHistory($activeChatPeerId);
    }
  }

  function formatTime(unixSecs: number): string {
    const d = new Date(unixSecs * 1000);
    return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  }

  function copyCodeSnippet(text: string, id: string) {
    navigator.clipboard.writeText(text);
    copiedMessageId = id;
    setTimeout(() => {
      copiedMessageId = '';
    }, 1800);
  }
</script>

<div class="flex h-full w-full bg-slate-900/40 border border-slate-800/80 rounded-2xl overflow-hidden backdrop-blur-xl">
  <!-- Left Column: Peer / Contacts Sidebar -->
  <aside class="w-72 sm:w-80 border-r border-slate-800/80 flex flex-col bg-slate-950/60 shrink-0">
    <!-- Search Bar -->
    <div class="p-3.5 border-b border-slate-800/80">
      <div class="relative">
        <Search class="w-4 h-4 text-slate-500 absolute left-3 top-1/2 -translate-y-1/2" />
        <input
          type="text"
          bind:value={searchQuery}
          placeholder="Search contacts..."
          class="w-full pl-9 pr-3 py-1.5 rounded-xl bg-slate-900 border border-slate-800 text-xs text-white placeholder-slate-500 focus:outline-none focus:border-cyan-500/60 transition"
        />
      </div>
    </div>

    <!-- Peer List -->
    <div class="flex-1 overflow-y-auto p-2 space-y-1">
      {#if filteredPeers.length === 0}
        <div class="p-6 text-center text-xs text-slate-500">
          <User class="w-8 h-8 mx-auto mb-2 text-slate-600 stroke-[1.5]" />
          <p>No contacts found.</p>
          <p class="text-[11px] text-slate-600 mt-1">Connect peers via LAN Radar or add contacts by email.</p>
        </div>
      {:else}
        {#each filteredPeers as peer (peer.id)}
          {@const isSelected = $activeChatPeerId === peer.id}
          {@const unread = $unreadCounts[peer.id] || 0}
          <button
            type="button"
            on:click={() => {
              activeChatPeerId.set(peer.id);
            }}
            class="w-full text-left p-3 rounded-xl transition-all flex items-center justify-between gap-2.5 {isSelected
              ? 'bg-gradient-to-r from-cyan-500/20 to-indigo-500/20 border border-cyan-500/40 shadow-sm shadow-cyan-500/10'
              : 'hover:bg-slate-900/80 border border-transparent'}"
          >
            <div class="flex items-center gap-3 min-w-0">
              <!-- Avatar / Status Ring -->
              <div class="relative shrink-0">
                <div class="w-9 h-9 rounded-xl bg-slate-800 border border-slate-700 flex items-center justify-center font-bold text-xs text-cyan-300">
                  {peer.name.slice(0, 2).toUpperCase()}
                </div>
                {#if peer.isOnline}
                  <span class="absolute -bottom-0.5 -right-0.5 w-3 h-3 rounded-full bg-emerald-500 border-2 border-slate-950 shadow-sm shadow-emerald-500/50"></span>
                {:else}
                  <span class="absolute -bottom-0.5 -right-0.5 w-3 h-3 rounded-full bg-slate-600 border-2 border-slate-950"></span>
                {/if}
              </div>

              <!-- Name & Subtext -->
              <div class="min-w-0">
                <div class="flex items-center gap-1.5">
                  <span class="text-xs font-bold text-white truncate max-w-[130px] sm:max-w-[150px]">
                    {peer.name}
                  </span>
                  {#if peer.isOwnDevice}
                    <span class="text-[9px] px-1 rounded bg-indigo-500/20 text-indigo-300 font-mono">You</span>
                  {/if}
                </div>
                <div class="flex items-center gap-1.5 text-[11px] text-slate-400 mt-0.5">
                  {#if peer.networkType === 'lan'}
                    <Wifi class="w-3 h-3 text-cyan-400 shrink-0" />
                    <span>LAN</span>
                  {:else}
                    <Globe class="w-3 h-3 text-indigo-400 shrink-0" />
                    <span>WAN</span>
                  {/if}
                  <span>•</span>
                  <span class={peer.isOnline ? 'text-emerald-400' : 'text-slate-500'}>
                    {peer.isOnline ? 'Online' : 'Offline'}
                  </span>
                </div>
              </div>
            </div>

            <!-- Unread Badge -->
            {#if unread > 0}
              <span class="px-2 py-0.5 rounded-full bg-cyan-400 text-slate-950 font-bold text-[10px] font-mono shadow-md shadow-cyan-400/30 shrink-0 animate-pulse">
                {unread}
              </span>
            {/if}
          </button>
        {/each}
      {/if}
    </div>
  </aside>

  <!-- Right Column: Main Chat & Stream -->
  <section class="flex-1 flex flex-col bg-slate-950/40 relative min-w-0">
    {#if !activePeer}
      <div class="flex-1 flex flex-col items-center justify-center text-center p-8">
        <div class="w-16 h-16 rounded-2xl bg-cyan-500/10 border border-cyan-500/30 flex items-center justify-center mb-4 text-cyan-400 shadow-xl shadow-cyan-500/10">
          <MessageSquare class="w-8 h-8 stroke-[1.8]" />
        </div>
        <h3 class="text-base font-bold text-white font-display">P2P Encrypted Chat</h3>
        <p class="text-xs text-slate-400 mt-1.5 max-w-sm">
          Select a contact or device from the left to start sending messages, plans, code snippets, and direct file packages.
        </p>
      </div>
    {:else}
      <!-- Chat Header -->
      <header class="p-3.5 px-5 border-b border-slate-800/80 bg-slate-950/70 flex items-center justify-between gap-3 shrink-0">
        <div class="flex items-center gap-3 min-w-0">
          <div class="relative shrink-0">
            <div class="w-9 h-9 rounded-xl bg-slate-800 border border-slate-700 flex items-center justify-center font-bold text-xs text-cyan-300">
              {activePeer.name.slice(0, 2).toUpperCase()}
            </div>
            {#if activePeer.isOnline}
              <span class="absolute -bottom-0.5 -right-0.5 w-3 h-3 rounded-full bg-emerald-500 border-2 border-slate-950 shadow-sm shadow-emerald-500/50"></span>
            {/if}
          </div>

          <div class="min-w-0">
            <div class="flex items-center gap-2">
              <h2 class="text-xs font-bold text-white truncate">{activePeer.name}</h2>
              <span class="text-[10px] px-1.5 py-0.2 rounded bg-slate-800 text-slate-300 font-mono">
                {activePeer.networkType.toUpperCase()}
              </span>
            </div>
            <p class="text-[11px] text-slate-400 truncate font-mono">
              {activePeer.email || activePeer.id}
            </p>
          </div>
        </div>

        <div class="flex items-center gap-2">
          <button
            type="button"
            on:click={handleClearHistory}
            class="p-2 rounded-xl text-slate-400 hover:text-rose-400 hover:bg-slate-900 border border-transparent hover:border-slate-800 transition"
            title="Clear Chat History with this peer"
          >
            <Trash2 class="w-4 h-4" />
          </button>
        </div>
      </header>

      <!-- Message History Stream -->
      <div
        bind:this={chatScrollContainer}
        class="flex-1 overflow-y-auto p-4 space-y-3.5 scroll-smooth"
      >
        {#if activeMessages.length === 0}
          <div class="h-full flex flex-col items-center justify-center text-center p-8">
            <div class="w-12 h-12 rounded-2xl bg-cyan-500/10 border border-cyan-500/30 flex items-center justify-center text-cyan-400 mb-3">
              <Sparkles class="w-6 h-6 stroke-[1.8]" />
            </div>
            <h4 class="text-sm font-bold text-white">Start Encrypted Conversation</h4>
            <p class="text-xs text-slate-400 mt-1 max-w-xs leading-relaxed">
              Messages are sent directly (LAN) or via E2EE relay (WAN). Share markdown plans, code blocks, or attach files.
            </p>
          </div>
        {:else}
          {#each activeMessages as msg (msg.id)}
            {@const isOutgoing = msg.direction === 'outgoing'}
            {@const files = parseFilesJson(msg.files_json)}
            {@const linkedTransfer = getLinkedTransfer(msg.transfer_session_id)}

            <div class="flex flex-col {isOutgoing ? 'items-end' : 'items-start'}">
              <div
                class="max-w-[85%] sm:max-w-[75%] rounded-2xl p-3.5 text-xs shadow-lg transition-all {isOutgoing
                  ? 'bg-gradient-to-tr from-cyan-950/60 to-indigo-950/60 border border-cyan-500/40 text-slate-100 rounded-br-sm shadow-cyan-500/5'
                  : 'bg-slate-900/90 border border-slate-800 text-slate-200 rounded-bl-sm'}"
              >
                <!-- Markdown Content -->
                {#if msg.content}
                  <div class="prose prose-invert prose-xs max-w-none text-slate-200 leading-relaxed font-sans break-words markdown-body">
                    {@html renderMarkdown(msg.content)}
                  </div>
                {/if}

                <!-- Integrated File / Media Transfer Card -->
                {#if files.length > 0}
                  <div class="mt-2.5 space-y-2 pt-2 border-t border-slate-800/80">
                    <div class="flex items-center justify-between text-[11px] font-mono text-slate-400">
                      <span class="flex items-center gap-1 text-cyan-300 font-semibold">
                        <Paperclip class="w-3 h-3 text-cyan-400" />
                        {files.length} Shared File(s)
                      </span>
                      {#if linkedTransfer}
                        <span class="text-xs {linkedTransfer.status === 'completed' ? 'text-emerald-400 font-bold' : 'text-cyan-400'}">
                          {linkedTransfer.status === 'completed' ? 'Verified' : `${linkedTransfer.status}`}
                        </span>
                      {/if}
                    </div>

                    <div class="space-y-1.5">
                      {#each files as f}
                        {@const badge = getFileTypeBadge(f.name)}
                        <div class="flex items-center justify-between gap-2 p-2 rounded-xl bg-slate-950/80 border border-slate-800/90">
                          <div class="flex items-center gap-2.5 min-w-0">
                            <div class="w-7 h-7 rounded-lg {badge.color} border flex items-center justify-center font-mono font-bold text-[9px] shrink-0">
                              {badge.label}
                            </div>
                            <div class="min-w-0">
                              <p class="text-xs font-semibold text-white truncate max-w-[180px] sm:max-w-xs" title={f.name}>
                                {f.name}
                              </p>
                              {#if f.size > 0}
                                <p class="text-[10px] font-mono text-slate-400">{formatBytes(f.size)}</p>
                              {/if}
                            </div>
                          </div>

                          <!-- Action Buttons on File Card -->
                          <div class="flex items-center gap-1.5 shrink-0">
                            {#if f.path}
                              <button
                                type="button"
                                on:click={() => handleOpenFile(f.path)}
                                class="px-2 py-0.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-[11px] text-slate-200 hover:text-white border border-slate-700 transition"
                                title="Open File"
                              >
                                Open
                              </button>
                              <button
                                type="button"
                                on:click={() => handleOpenFolder(f.path)}
                                class="p-1 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-400 hover:text-amber-300 border border-slate-700 transition"
                                title="Open Folder"
                              >
                                <FolderOpen class="w-3 h-3" />
                              </button>
                            {:else if linkedTransfer?.target_path}
                              <button
                                type="button"
                                on:click={() => handleOpenFile(linkedTransfer.target_path || '')}
                                class="px-2 py-0.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-[11px] text-slate-200 hover:text-white border border-slate-700 transition"
                                title="Open File"
                              >
                                Open
                              </button>
                              <button
                                type="button"
                                on:click={() => handleOpenFolder(linkedTransfer.target_path || '')}
                                class="p-1 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-400 hover:text-amber-300 border border-slate-700 transition"
                                title="Open Folder"
                              >
                                <FolderOpen class="w-3 h-3" />
                              </button>
                            {/if}
                          </div>
                        </div>
                      {/each}
                    </div>

                    <!-- Live Transfer Progress in Chat Bubble -->
                    {#if linkedTransfer && linkedTransfer.status === 'in_progress'}
                      {@const pct = linkedTransfer.total_bytes > 0
                        ? Math.min(100, Math.round((linkedTransfer.transferred_bytes / linkedTransfer.total_bytes) * 100))
                        : 0}
                      <div class="space-y-1 pt-1">
                        <div class="w-full h-1.5 rounded-full bg-slate-950 overflow-hidden border border-slate-800">
                          <div
                            class="h-full bg-gradient-to-r from-cyan-400 to-indigo-500 transition-all duration-150"
                            style="width: {pct}%"
                          ></div>
                        </div>
                        <div class="flex items-center justify-between text-[10px] font-mono text-slate-400">
                          <span>{pct}% ({formatBytes(linkedTransfer.transferred_bytes)} / {formatBytes(linkedTransfer.total_bytes)})</span>
                          <span class="text-cyan-400">{(linkedTransfer.speed_mbps || 0).toFixed(1)} MB/s</span>
                        </div>
                      </div>
                    {/if}
                  </div>
                {/if}

                <!-- Timestamp & Delivery Indicator -->
                <div class="flex items-center justify-end gap-1.5 mt-1.5 pt-1 text-[10px] font-mono text-slate-500">
                  <span>{formatTime(msg.created_at)}</span>
                  {#if isOutgoing}
                    <CheckCheck class="w-3 h-3 text-cyan-400" />
                  {/if}
                </div>
              </div>
            </div>
          {/each}
        {/if}
      </div>

      <!-- Attachment Preview Pills -->
      {#if attachingFiles.length > 0}
        <div class="px-4 py-2 border-t border-slate-800/80 bg-slate-950/80 flex items-center gap-2 overflow-x-auto">
          <span class="text-[11px] font-mono text-slate-400 shrink-0">Attached:</span>
          {#each attachingFiles as fp, idx}
            <div class="flex items-center gap-1.5 px-2.5 py-1 rounded-xl bg-slate-900 border border-slate-700/80 text-xs text-white shrink-0">
              <FileText class="w-3.5 h-3.5 text-cyan-400 shrink-0" />
              <span class="truncate max-w-[150px]">{getBaseName(fp)}</span>
              <button
                type="button"
                on:click={() => removeAttachedFile(idx)}
                class="text-slate-400 hover:text-rose-400 p-0.5 rounded transition"
              >
                <X class="w-3 h-3" />
              </button>
            </div>
          {/each}
        </div>
      {/if}

      <!-- Chat Input Area -->
      <footer class="p-3.5 border-t border-slate-800/80 bg-slate-950/80 shrink-0">
        <form
          on:submit|preventDefault={handleSendMessage}
          class="flex items-end gap-2"
        >
          <!-- Attachment Dropdown / Actions -->
          <div class="flex items-center gap-1 shrink-0 pb-1">
            <button
              type="button"
              on:click={handlePickFiles}
              class="p-2 rounded-xl text-slate-400 hover:text-cyan-300 hover:bg-slate-900 border border-transparent hover:border-slate-800 transition"
              title="Attach Multiple Files (Images, Videos, Code, PDFs, Docs, etc.)"
            >
              <Paperclip class="w-4 h-4" />
            </button>
            <button
              type="button"
              on:click={handlePickFolder}
              class="p-2 rounded-xl text-slate-400 hover:text-cyan-300 hover:bg-slate-900 border border-transparent hover:border-slate-800 transition"
              title="Attach Entire Folder"
            >
              <FolderUp class="w-4 h-4" />
            </button>
          </div>

          <!-- Input Textarea -->
          <div class="flex-1 relative">
            <textarea
              bind:value={messageInput}
              on:keydown={handleKeyDown}
              rows="1"
              placeholder="Type message, plans, code (Markdown supported)..."
              class="w-full px-4 py-2.5 rounded-xl bg-slate-900 border border-slate-800 text-xs text-white placeholder-slate-500 focus:outline-none focus:border-cyan-500/60 focus:ring-1 focus:ring-cyan-500/30 resize-none max-h-32 transition font-sans"
            ></textarea>
          </div>

          <!-- Send Button -->
          <button
            type="submit"
            disabled={isSending || (!messageInput.trim() && attachingFiles.length === 0)}
            class="px-4 py-2.5 rounded-xl bg-gradient-to-r from-cyan-500 to-indigo-600 hover:from-cyan-400 hover:to-indigo-500 disabled:opacity-40 disabled:cursor-not-allowed text-white font-bold text-xs shadow-lg shadow-cyan-500/20 transition-all flex items-center gap-1.5 shrink-0"
          >
            {#if isSending}
              <RefreshCw class="w-3.5 h-3.5 animate-spin" />
            {:else}
              <Send class="w-3.5 h-3.5" />
            {/if}
            <span>Send</span>
          </button>
        </form>

        <div class="flex items-center justify-between text-[10px] text-slate-500 font-mono mt-2 px-1">
          <div class="flex items-center gap-2">
            <span>⚡ Markdown: `code`, **bold**, *italic*, - list</span>
            <span>•</span>
            <span>Encrypted P2P</span>
          </div>
          <span>Shift+Enter for new line</span>
        </div>
      </footer>
    {/if}
  </section>
</div>

<style>
  :global(.markdown-body pre) {
    background-color: #020617 !important;
    border: 1px solid #1e293b;
    border-radius: 0.75rem;
    padding: 0.75rem 1rem;
    overflow-x: auto;
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 0.75rem;
    margin: 0.5rem 0;
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
    margin-bottom: 0.35rem;
  }
  :global(.markdown-body p:last-child) {
    margin-bottom: 0;
  }
  :global(.markdown-body ul, .markdown-body ol) {
    padding-left: 1.25rem;
    margin: 0.35rem 0;
  }
  :global(.markdown-body blockquote) {
    border-left: 3px solid #06b6d4;
    padding-left: 0.75rem;
    color: #94a3b8;
    margin: 0.35rem 0;
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

