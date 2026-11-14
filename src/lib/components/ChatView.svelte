<script lang="ts">
  import { onMount, tick, onDestroy } from 'svelte';
  import { open } from '@tauri-apps/plugin-dialog';
  import { convertFileSrc } from '@tauri-apps/api/core';
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
    ArrowDownLeft,
    ArrowLeft,
    Smile,
    Copy,
    Shield,
    ShieldAlert,
    ShieldCheck,
    Ban,
    Image as ImageIcon,
    Clipboard,
  } from 'lucide-svelte';
  import { contacts, lanContacts } from '../stores/contacts';
  import { cloudContacts, cloudMyDevices, activeChatPeerId, appSettings, incomingRequestQueue, activeMainTab } from '../stores/state';
  import {
    chatStore,
    unreadCounts,
    hasMoreMap,
    isLoadingOlder,
    loadChatHistory,
    loadOlderMessages,
    markChatRead,
    clearChatHistory,
    handleOutgoingChatMessage,
    formatRelativeTime
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
  let textareaEl: HTMLTextAreaElement;
  let isSending = false;
  let copiedMessageId = '';
  let attachingFiles: string[] = [];
  let showEmojiPicker = false;
  let relativeTimeTicker = 0;
  let localThumbnails: Record<string, string> = {};

  async function ensureLocalThumbnail(path: string | null) {
    if (!path || localThumbnails[path]) return;
    try {
      const thumb = await raydropApi.getFileThumbnail(path);
      if (thumb) {
        localThumbnails = { ...localThumbnails, [path]: thumb };
      }
    } catch (_) {
      // ignore
    }
  }

  // Update relative time strings every 15 seconds
  const tickerInterval = setInterval(() => {
    relativeTimeTicker += 1;
  }, 15000);

  onDestroy(() => {
    clearInterval(tickerInterval);
  });

  // Emojis for picker
  const emojiCategories = [
    {
      name: 'Smileys',
      emojis: ['😀', '😂', '🤣', '😊', '😍', '😎', '🤔', '🤫', '🥳', '🥺', '😭', '😇', '😴'],
    },
    {
      name: 'Gestures',
      emojis: ['👍', '👎', '👏', '🙌', '🤝', '🙏', '✌️', '🚀', '🔥', '💯', '⚡', '❤️', '💡'],
    },
    {
      name: 'Tech & Work',
      emojis: ['💻', '📱', '🖥️', '📂', '📁', '📄', '🔒', '🛡️', '🌐', '📶', '⚙️', '☕', '✅', '❌'],
    },
  ];

  onMount(async () => {
    try {
      const loaded = await raydropApi.getContacts();
      if (loaded && loaded.length > 0) {
        contacts.set(loaded);
      }
    } catch (_) {}

    if ($activeChatPeerId) {
      await loadChatHistory($activeChatPeerId);
      await tick();
      scrollToBottom();
    }
  });

  // Comprehensive Peer List: Combines local SQLite contacts, LAN mDNS, Cloud Devices & Contacts
  $: allPeers = (() => {
    const peerMap = new Map<string, any>();

    for (const c of $contacts) {
      peerMap.set(c.device_id, {
        id: c.device_id,
        name: c.alias_name || c.user_name || 'Raydrop Peer',
        email: c.user_email || '',
        osType: c.os_type || 'windows',
        isOnline: c.is_online,
        networkType: c.network_type || 'wan',
        isOwnDevice: c.is_own_device,
        trustLevel: c.trust_level || 'ask',
      });
    }

    for (const dev of $cloudMyDevices) {
      if (dev.device_id === $appSettings.device_id) continue;
      if (!peerMap.has(dev.device_id)) {
        peerMap.set(dev.device_id, {
          id: dev.device_id,
          name: dev.device_name || 'My Device',
          email: dev.user_email || '',
          osType: dev.os_type || 'windows',
          isOnline: dev.is_online,
          networkType: 'wan',
          isOwnDevice: true,
          trustLevel: 'trusted_auto_accept',
        });
      } else {
        const existing = peerMap.get(dev.device_id);
        existing.isOnline = existing.isOnline || dev.is_online;
      }
    }

    for (const group of $cloudContacts) {
      for (const dev of group.devices) {
        if (!peerMap.has(dev.device_id)) {
          peerMap.set(dev.device_id, {
            id: dev.device_id,
            name: group.name ? `${group.name} (${dev.device_name})` : dev.device_name,
            email: group.email || '',
            osType: dev.os_type || 'windows',
            isOnline: dev.is_online,
            networkType: 'wan',
            isOwnDevice: false,
            trustLevel: group.trust_level || 'ask',
          });
        } else {
          const existing = peerMap.get(dev.device_id);
          existing.isOnline = existing.isOnline || dev.is_online;
        }
      }
    }

    return Array.from(peerMap.values());
  })();

  $: filteredPeers = allPeers.filter((p) => {
    if (!searchQuery.trim()) return true;
    const q = searchQuery.toLowerCase();
    return p.name.toLowerCase().includes(q) || p.email.toLowerCase().includes(q);
  });

  // Always resolve a valid activePeer if activeChatPeerId is selected to prevent blank screens
  $: activePeer =
    allPeers.find((p) => p.id === $activeChatPeerId) ||
    ($activeChatPeerId
      ? {
          id: $activeChatPeerId,
          name: `Peer (${$activeChatPeerId.slice(0, 10)}...)`,
          email: '',
          osType: 'windows',
          isOnline: false,
          networkType: 'wan',
          isOwnDevice: false,
          trustLevel: 'ask',
        }
      : filteredPeers[0] || null);

  $: activeMessages = ($activeChatPeerId ? $chatStore[$activeChatPeerId] : []) || [];
  $: hasMore = $activeChatPeerId ? !!$hasMoreMap[$activeChatPeerId] : false;
  $: isPeerBlocked = activePeer?.trustLevel === 'blocked';

  // Reactive Map of all transfers for 100% reactive progress updates
  $: transferMap = new Map($transfers.map((t) => [t.id, t]));

  // Auto-select first peer if none selected
  $: if (!$activeChatPeerId && filteredPeers.length > 0) {
    activeChatPeerId.set(filteredPeers[0].id);
  }

  // When active peer changes or user switches to 'chat' tab, load chat history, mark read and scroll to bottom
  let prevPeerId: string | null = null;
  $: if ($activeChatPeerId && $activeChatPeerId !== prevPeerId) {
    prevPeerId = $activeChatPeerId;
    loadChatHistory($activeChatPeerId).then(() => {
      tick().then(() => {
        scrollToBottom();
        setTimeout(scrollToBottom, 60);
      });
    });
  }

  // Reactive tab focus handler: auto-scroll when user returns to 'chat' tab
  let prevTab = '';
  $: if ($activeMainTab === 'chat' && prevTab !== 'chat') {
    prevTab = 'chat';
    if ($activeChatPeerId) {
      markChatRead($activeChatPeerId);
      tick().then(() => {
        scrollToBottom();
        setTimeout(scrollToBottom, 60);
      });
    }
  } else if ($activeMainTab !== 'chat') {
    prevTab = $activeMainTab;
  }

  // Auto-scroll to bottom when new messages arrive (both incoming & outgoing)
  let prevMsgCount = 0;
  $: if (activeMessages.length !== prevMsgCount) {
    const isNew = activeMessages.length > prevMsgCount;
    prevMsgCount = activeMessages.length;
    if (isNew) {
      tick().then(() => {
        scrollToBottom();
        setTimeout(scrollToBottom, 60);
      });
    }
  }

  function scrollToBottom() {
    if (chatScrollContainer) {
      chatScrollContainer.scrollTop = chatScrollContainer.scrollHeight;
    }
  }

  // Infinite Scroll Pagination: load older messages when user scrolls near top
  async function handleScroll() {
    if (!chatScrollContainer || !$activeChatPeerId || $isLoadingOlder || !hasMore) return;
    if (chatScrollContainer.scrollTop < 60) {
      const prevScrollHeight = chatScrollContainer.scrollHeight;
      const loadedCount = await loadOlderMessages($activeChatPeerId);
      if (loadedCount > 0) {
        await tick();
        chatScrollContainer.scrollTop = chatScrollContainer.scrollHeight - prevScrollHeight;
      }
    }
  }

  function getBaseName(path: string): string {
    const parts = path.split(/[/\\]/);
    return parts[parts.length - 1] || path;
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

  function getFileTypeBadge(filename: string): { label: string; color: string; isImage: boolean } {
    const ext = filename.split('.').pop()?.toUpperCase() || 'FILE';
    const isImage = ['JPG', 'JPEG', 'PNG', 'WEBP', 'SVG', 'GIF', 'BMP', 'ICO'].includes(ext);
    if (['SQL', 'DB', 'SQLITE'].includes(ext)) {
      return { label: ext, color: 'bg-emerald-500/20 text-emerald-300 border-emerald-500/40', isImage: false };
    }
    if (['ZIP', 'RAR', '7Z', 'TAR', 'GZ'].includes(ext)) {
      return { label: ext, color: 'bg-amber-500/20 text-amber-300 border-amber-500/40', isImage: false };
    }
    if (['MP4', 'MKV', 'MOV', 'AVI', 'WEBM'].includes(ext)) {
      return { label: ext, color: 'bg-purple-500/20 text-purple-300 border-purple-500/40', isImage: false };
    }
    if (isImage) {
      return { label: ext, color: 'bg-pink-500/20 text-pink-300 border-pink-500/40', isImage: true };
    }
    if (['PDF', 'DOC', 'DOCX', 'XLS', 'XLSX', 'PPTX'].includes(ext)) {
      return { label: ext, color: 'bg-blue-500/20 text-blue-300 border-blue-500/40', isImage: false };
    }
    if (['RS', 'TS', 'JS', 'PY', 'GO', 'CPP', 'C', 'HTML', 'CSS', 'JSON', 'SH'].includes(ext)) {
      return { label: ext, color: 'bg-cyan-500/20 text-cyan-300 border-cyan-500/40', isImage: false };
    }
    return { label: ext.slice(0, 4), color: 'bg-slate-800 text-slate-300 border-slate-700', isImage: false };
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

  // Resolve accurate local file path for both sender and recipient
  function resolveLocalFilePath(
    f: ChatFileItem,
    isOutgoing: boolean,
    linkedTransfer?: TransferRecord,
    fileIdx?: number
  ): string | null {
    if (isOutgoing && f.path) {
      return f.path;
    }

    if (linkedTransfer) {
      if (linkedTransfer.source_paths) {
        const paths = parseFilePaths(linkedTransfer.source_paths);
        const match = paths.find(
          (p) => getBaseName(p).toLowerCase() === f.name.toLowerCase()
        );
        if (match) return match;
        if (fileIdx !== undefined && paths[fileIdx]) {
          return paths[fileIdx];
        }
        if (paths.length === 1) {
          return paths[0];
        }
      }

      const dir = linkedTransfer.target_path || $appSettings.download_dir;
      if (dir) {
        const sep = dir.includes('/') ? '/' : '\\';
        return `${dir.replace(/[/\\]$/, '')}${sep}${f.name}`;
      }
    }

    if ($appSettings.download_dir) {
      const sep = $appSettings.download_dir.includes('/') ? '/' : '\\';
      return `${$appSettings.download_dir.replace(/[/\\]$/, '')}${sep}${f.name}`;
    }

    return null;
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
      const isMobile = /Android|iPhone|iPad/i.test(navigator.userAgent);
      if (isMobile) {
        // Fallback to picking multiple files on mobile
        return handlePickFiles();
      }
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

  function insertEmoji(emoji: string) {
    if (!textareaEl) {
      messageInput += emoji;
      return;
    }
    const start = textareaEl.selectionStart || 0;
    const end = textareaEl.selectionEnd || 0;
    messageInput = messageInput.slice(0, start) + emoji + messageInput.slice(end);
    tick().then(() => {
      textareaEl.focus();
      textareaEl.selectionStart = textareaEl.selectionEnd = start + emoji.length;
      autoResizeTextarea();
    });
  }

  function autoResizeTextarea() {
    if (!textareaEl) return;
    textareaEl.style.height = 'auto';
    const newH = Math.min(textareaEl.scrollHeight, 140);
    textareaEl.style.height = `${Math.max(38, newH)}px`;
  }

  async function handleSendMessage() {
    if (!$activeChatPeerId || isPeerBlocked || isSending) return;
    const text = (messageInput || '').trim();
    if (!text && attachingFiles.length === 0) return;

    isSending = true;
    showEmojiPicker = false;
    try {
      let transferSessionId: string | null = null;
      let filesJson: string | null = null;
      let msgType = 'text';

      const filesToAttach = [...attachingFiles];

      if (filesToAttach.length > 0) {
        msgType = 'file_transfer';
        // 1. Initiate accelerated P2P transfer session in Raydrop backend
        transferSessionId = await raydropApi.initiateTransfer(
          $activeChatPeerId,
          filesToAttach
        );

        const filesMeta: ChatFileItem[] = filesToAttach.map((fp) => ({
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
      if (textareaEl) {
        textareaEl.style.height = 'auto';
      }
      await tick();
      scrollToBottom();
    } catch (err) {
      console.error('Failed to send message:', err);
    } finally {
      isSending = false;
    }
  }

  async function handleAcceptChatTransfer(sessionId: string) {
    try {
      await raydropApi.respondTransferRequest(
        sessionId,
        true,
        $appSettings.download_dir
      );
      incomingRequestQueue.update((q) => q.filter((item) => item.session_id !== sessionId));
    } catch (err) {
      console.warn('Failed to accept transfer from chat:', err);
    }
  }

  async function handleDeclineChatTransfer(sessionId: string) {
    try {
      await raydropApi.respondTransferRequest(sessionId, false, null);
      incomingRequestQueue.update((q) => q.filter((item) => item.session_id !== sessionId));
    } catch (err) {
      console.warn('Failed to decline transfer from chat:', err);
    }
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSendMessage();
    }
  }

  async function handleOpenFile(path: string | null) {
    if (!path) return;
    try {
      await raydropApi.openFilePath(path);
    } catch (err) {
      console.warn('Failed to open file:', err);
    }
  }

  async function handleOpenFolder(path: string | null) {
    if (!path) return;
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

  async function toggleBlockPeer() {
    if (!activePeer) return;
    const newTrust = isPeerBlocked ? 'ask' : 'blocked';
    try {
      await raydropApi.updateContactTrust(
        activePeer.id,
        newTrust,
        activePeer.email || null
      );
      contacts.set(await raydropApi.getContacts());
    } catch (err) {
      console.warn('Failed to update peer trust:', err);
    }
  }

  function copyMessageText(content: string, id: string) {
    navigator.clipboard.writeText(content);
    copiedMessageId = id;
    setTimeout(() => {
      copiedMessageId = '';
    }, 1800);
  }

  // Intercept links in Markdown to open via system browser instead of webview
  async function handleMarkdownClick(e: MouseEvent) {
    const target = (e.target as HTMLElement)?.closest('a');
    if (!target) return;
    const href = target.getAttribute('href');
    if (href && (href.startsWith('http://') || href.startsWith('https://'))) {
      e.preventDefault();
      await raydropApi.openExternalUrl(href);
    }
  }
</script>

<div class="flex h-full w-full bg-slate-900/40 border border-slate-800/80 rounded-2xl overflow-hidden backdrop-blur-xl">
  <!-- Left Column: Peer / Contacts Sidebar -->
  <aside class="w-full md:w-80 border-r border-slate-800/80 flex flex-col bg-slate-950/60 shrink-0 {$activeChatPeerId ? 'hidden md:flex' : 'flex'}">
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
          {@const blocked = peer.trustLevel === 'blocked'}

          <button
            type="button"
            on:click={() => {
              activeChatPeerId.set(peer.id);
            }}
            class="w-full text-left p-3 rounded-xl transition-all flex items-center justify-between gap-2.5 {isSelected
              ? 'bg-gradient-to-r from-cyan-500/20 to-indigo-500/20 border border-cyan-500/40 shadow-sm shadow-cyan-500/10'
              : 'hover:bg-slate-900/80 border border-transparent'} {blocked ? 'opacity-60' : ''}"
          >
            <div class="flex items-center gap-3 min-w-0">
              <!-- Avatar / Status Ring -->
              <div class="relative shrink-0">
                <div class="w-9 h-9 rounded-xl bg-slate-800 border border-slate-700 flex items-center justify-center font-bold text-xs text-cyan-300">
                  {peer.name.slice(0, 2).toUpperCase()}
                </div>
                {#if blocked}
                  <span class="absolute -bottom-0.5 -right-0.5 w-3 h-3 rounded-full bg-rose-500 border-2 border-slate-950 flex items-center justify-center">
                    <span class="w-1.5 h-0.5 bg-white"></span>
                  </span>
                {:else if peer.isOnline}
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
                  {#if blocked}
                    <span class="text-[9px] px-1 rounded bg-rose-500/20 text-rose-300 font-mono">Blocked</span>
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
  <section class="flex-1 flex flex-col bg-slate-950/40 relative min-w-0 {!$activeChatPeerId ? 'hidden md:flex' : 'flex'}">
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
      <header class="p-3 sm:p-3.5 px-3.5 sm:px-5 border-b border-slate-800/80 bg-slate-950/70 flex items-center justify-between gap-2 sm:gap-3 shrink-0">
        <div class="flex items-center gap-2 sm:gap-3 min-w-0">
          <!-- Back button on mobile -->
          <button
            type="button"
            on:click={() => activeChatPeerId.set(null)}
            class="md:hidden p-1.5 -ml-1 rounded-xl text-slate-400 hover:text-white hover:bg-slate-800 transition shrink-0"
            aria-label="Kembali ke kontak"
            title="Kembali ke kontak"
          >
            <ArrowLeft class="w-5 h-5" />
          </button>

          <div class="relative shrink-0">
            <div class="w-8 h-8 sm:w-9 sm:h-9 rounded-xl bg-slate-800 border border-slate-700 flex items-center justify-center font-bold text-xs text-cyan-300">
              {activePeer.name.slice(0, 2).toUpperCase()}
            </div>
            {#if activePeer.isOnline}
              <span class="absolute -bottom-0.5 -right-0.5 w-2.5 h-2.5 sm:w-3 sm:h-3 rounded-full bg-emerald-500 border-2 border-slate-950 shadow-sm shadow-emerald-500/50"></span>
            {/if}
          </div>

          <div class="min-w-0">
            <div class="flex items-center gap-1.5 sm:gap-2">
              <h2 class="text-xs font-bold text-white truncate max-w-[120px] sm:max-w-none">{activePeer.name}</h2>
              <span class="text-[9px] sm:text-[10px] px-1 sm:px-1.5 py-0.2 rounded bg-slate-800 text-slate-300 font-mono">
                {activePeer.networkType.toUpperCase()}
              </span>
              {#if isPeerBlocked}
                <span class="text-[9px] sm:text-[10px] px-1 sm:px-1.5 py-0.2 rounded bg-rose-500/20 text-rose-300 font-mono font-bold">
                  BLOCKED
                </span>
              {/if}
            </div>
            <p class="text-[10px] sm:text-[11px] text-slate-400 truncate font-mono">
              {activePeer.email || activePeer.id}
            </p>
          </div>
        </div>

        <div class="flex items-center gap-1 sm:gap-1.5 shrink-0">
          <!-- Block / Unblock Button -->
          <button
            type="button"
            on:click={toggleBlockPeer}
            class="px-2 sm:px-2.5 py-1 rounded-xl text-xs font-semibold border transition flex items-center gap-1 {isPeerBlocked
              ? 'bg-emerald-500/15 hover:bg-emerald-500/25 text-emerald-300 border-emerald-500/30'
              : 'bg-rose-500/10 hover:bg-rose-500/20 text-rose-400 border-rose-500/20'}"
            title={isPeerBlocked ? 'Unblock this peer' : 'Block this peer'}
          >
            {#if isPeerBlocked}
              <ShieldCheck class="w-3.5 h-3.5" />
              <span class="hidden sm:inline">Unblock</span>
            {:else}
              <Ban class="w-3.5 h-3.5" />
              <span class="hidden sm:inline">Block</span>
            {/if}
          </button>

          <!-- Clear Chat History -->
          <button
            type="button"
            on:click={handleClearHistory}
            class="p-1.5 rounded-xl text-slate-400 hover:text-rose-400 hover:bg-slate-900 border border-transparent hover:border-slate-800 transition"
            title="Clear Chat History with this peer"
          >
            <Trash2 class="w-4 h-4" />
          </button>
        </div>
      </header>

      <!-- Message History Stream with Infinite Scroll -->
      <!-- svelte-ignore a11y-click-events-have-key-events -->
      <!-- svelte-ignore a11y-no-static-element-interactions -->
      <div
        bind:this={chatScrollContainer}
        on:scroll={handleScroll}
        on:click={handleMarkdownClick}
        class="flex-1 overflow-y-auto p-4 space-y-3.5 scroll-smooth relative"
      >
        <!-- Pagination Loading Indicator at Top -->
        {#if $isLoadingOlder}
          <div class="flex items-center justify-center py-2 text-xs text-slate-400 gap-1.5 font-mono">
            <RefreshCw class="w-3.5 h-3.5 animate-spin text-cyan-400" />
            <span>Loading older messages...</span>
          </div>
        {:else if hasMore}
          <div class="text-center py-1">
            <button
              type="button"
              on:click={() => loadOlderMessages(activePeer.id)}
              class="text-[11px] text-cyan-400/80 hover:text-cyan-300 hover:underline font-mono"
            >
              Scroll up or click to load older messages &uarr;
            </button>
          </div>
        {/if}

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
            {@const linkedTransfer = msg.transfer_session_id ? $transfers.find((t) => t.id === msg.transfer_session_id) : undefined}
            {@const pendingRequest = msg.transfer_session_id ? $incomingRequestQueue.find((q) => q.session_id === msg.transfer_session_id) : undefined}
            {@const hasContent = !!(msg.content && msg.content.trim().length > 0)}

            <div class="flex flex-col {isOutgoing ? 'items-end' : 'items-start'} group">
              <div
                class="max-w-[85%] sm:max-w-[75%] rounded-2xl p-3.5 text-xs shadow-lg transition-all relative {isOutgoing
                  ? 'bg-gradient-to-tr from-cyan-950/60 to-indigo-950/60 border border-cyan-500/40 text-slate-100 rounded-br-sm shadow-cyan-500/5'
                  : 'bg-slate-900/90 border border-slate-800 text-slate-200 rounded-bl-sm'}"
              >
                <!-- Quick Copy Button on Bubble Hover -->
                <button
                  type="button"
                  on:click={() => copyMessageText(msg.content, msg.id)}
                  class="absolute top-2 right-2 opacity-0 group-hover:opacity-100 p-1 rounded-md bg-slate-950/80 hover:bg-slate-800 text-slate-400 hover:text-cyan-300 border border-slate-800 transition-all cursor-pointer z-10"
                  title="Copy message"
                >
                  {#if copiedMessageId === msg.id}
                    <Check class="w-3 h-3 text-emerald-400" />
                  {:else}
                    <Copy class="w-3 h-3" />
                  {/if}
                </button>

                {#if msg.msg_type === 'clipboard'}
                  <div class="mb-1.5 flex items-center gap-1 text-[10px] font-mono font-bold text-indigo-400">
                    <Clipboard class="w-3 h-3" />
                    <span>Universal Clipboard</span>
                  </div>
                {/if}

                <!-- Markdown Content -->
                {#if hasContent}
                  <div class="text-slate-100 text-xs leading-relaxed font-sans break-words markdown-body select-text pr-6">
                    {@html renderMarkdown(msg.content)}
                  </div>
                {/if}

                <!-- Integrated File / Media Transfer Card -->
                {#if files.length > 0}
                  <div class="{hasContent ? 'mt-2.5 pt-2 border-t border-slate-800/80' : ''} space-y-2">
                    <div class="flex items-center justify-between text-[11px] font-mono text-slate-400">
                      <span class="flex items-center gap-1 text-cyan-300 font-semibold">
                        <Paperclip class="w-3 h-3 text-cyan-400" />
                        {files.length} Shared File(s)
                      </span>
                      {#if pendingRequest}
                        <span class="text-[11px] font-semibold text-amber-400 animate-pulse">
                          WAITING ACCEPT
                        </span>
                      {:else if linkedTransfer}
                        <span class="text-[11px] font-semibold {linkedTransfer.status === 'completed'
                          ? 'text-emerald-400'
                          : linkedTransfer.status === 'in_progress'
                          ? 'text-cyan-400 animate-pulse'
                          : 'text-amber-400'}">
                          {linkedTransfer.status === 'completed' ? 'Verified' : `${linkedTransfer.status.toUpperCase()}`}
                        </span>
                      {/if}
                    </div>

                    <div class="space-y-2">
                      {#each files as f, fileIdx}
                        {@const badge = getFileTypeBadge(f.name)}
                        {@const localPath = resolveLocalFilePath(f, isOutgoing, linkedTransfer, fileIdx)}
                        {@const isImageFile = badge.isImage}
                        {@const thumbSrc = f.thumbnail || (localPath ? localThumbnails[localPath] : null)}

                        <div class="p-2.5 rounded-xl bg-slate-950/80 border border-slate-800/90 space-y-2">
                          <!-- Direct In-Chat Image Thumbnail Preview (Fast & Lightweight) -->
                          {#if isImageFile}
                            {#if thumbSrc}
                              <!-- svelte-ignore a11y-click-events-have-key-events -->
                              <!-- svelte-ignore a11y-no-noninteractive-element-interactions -->
                              <div
                                on:click={() => localPath && handleOpenFile(localPath)}
                                class="rounded-lg overflow-hidden border border-slate-800/80 max-h-52 bg-slate-950 flex items-center justify-center {localPath ? 'cursor-pointer hover:border-cyan-500/40 transition group/img relative' : ''}"
                                title={localPath ? 'Click to open full image' : 'Image preview thumbnail'}
                              >
                                <img
                                  src={thumbSrc}
                                  alt={f.name}
                                  class="max-h-52 max-w-full object-contain rounded-lg shadow-sm"
                                  loading="lazy"
                                />
                              </div>
                            {:else if localPath}
                              <!-- Trigger background local thumbnail load -->
                              <span class="hidden">{ensureLocalThumbnail(localPath)}</span>
                            {/if}
                          {/if}

                          <div class="flex items-center justify-between gap-2">
                            <div class="flex items-center gap-2.5 min-w-0">
                              <div class="w-7 h-7 rounded-lg {badge.color} border flex items-center justify-center font-mono font-bold text-[9px] shrink-0">
                                {badge.label}
                              </div>
                              <div class="min-w-0">
                                <p class="text-xs font-semibold text-white truncate max-w-[170px] sm:max-w-xs" title={f.name}>
                                  {f.name}
                                </p>
                                {#if f.size > 0}
                                  <p class="text-[10px] font-mono text-slate-400">{formatBytes(f.size)}</p>
                                {/if}
                              </div>
                            </div>

                            <!-- Action Buttons on File Card -->
                            <div class="flex items-center gap-1.5 shrink-0">
                              {#if pendingRequest}
                                <button
                                  type="button"
                                  on:click={() => handleAcceptChatTransfer(pendingRequest.session_id)}
                                  class="px-2.5 py-1 rounded-lg bg-emerald-500/20 hover:bg-emerald-500/30 text-[11px] text-emerald-300 border border-emerald-500/40 font-bold transition shadow-sm"
                                  title="Accept & Download"
                                >
                                  Accept
                                </button>
                                <button
                                  type="button"
                                  on:click={() => handleDeclineChatTransfer(pendingRequest.session_id)}
                                  class="px-2 py-1 rounded-lg bg-rose-500/20 hover:bg-rose-500/30 text-[11px] text-rose-300 border border-rose-500/40 transition"
                                  title="Decline Transfer"
                                >
                                  Decline
                                </button>
                              {:else if linkedTransfer?.status === 'paused' && isOutgoing}
                                <button
                                  type="button"
                                  on:click={() => raydropApi.resumeTransfer(linkedTransfer.id)}
                                  class="px-2.5 py-1 rounded-lg bg-emerald-500/20 hover:bg-emerald-500/30 text-[11px] text-emerald-300 border border-emerald-500/40 font-bold transition"
                                  title="Resume transfer"
                                >
                                  Resume
                                </button>
                              {:else if localPath && (linkedTransfer?.status === 'completed' || isOutgoing)}
                                <button
                                  type="button"
                                  on:click={() => handleOpenFile(localPath)}
                                  class="px-2.5 py-1 rounded-lg bg-slate-800 hover:bg-slate-700 text-[11px] text-slate-200 hover:text-white border border-slate-700 transition"
                                  title="Open File Directly"
                                >
                                  Open
                                </button>
                                <button
                                  type="button"
                                  on:click={() => handleOpenFolder(localPath)}
                                  class="p-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-400 hover:text-amber-300 border border-slate-700 transition"
                                  title="Locate in Containing Folder"
                                >
                                  <FolderOpen class="w-3.5 h-3.5" />
                                </button>
                              {:else if linkedTransfer?.target_path}
                                <button
                                  type="button"
                                  on:click={() => handleOpenFolder(linkedTransfer?.target_path || '')}
                                  class="px-2.5 py-1 rounded-lg bg-slate-800 hover:bg-slate-700 text-[11px] text-slate-300 border border-slate-700 transition"
                                  title="Open Destination Folder"
                                >
                                  Folder
                                </button>
                              {/if}
                            </div>
                          </div>
                        </div>
                      {/each}
                    </div>

                    <!-- Live Transfer Progress in Chat Bubble (Reactive) -->
                    {#if linkedTransfer && linkedTransfer.status !== 'completed' && (linkedTransfer.status === 'in_progress' || (linkedTransfer.total_bytes > 0 && linkedTransfer.transferred_bytes < linkedTransfer.total_bytes))}
                      {@const totalBytes = linkedTransfer.total_bytes > 0 ? linkedTransfer.total_bytes : (files[0]?.size || 0)}
                      {@const transferredBytes = linkedTransfer.transferred_bytes || 0}
                      {@const pct = totalBytes > 0
                        ? Math.min(100, Math.round((transferredBytes / totalBytes) * 100))
                        : 0}
                      <div class="space-y-1 pt-1.5">
                        <div class="w-full h-2 rounded-full bg-slate-950 overflow-hidden border border-slate-800">
                          <div
                            class="h-full bg-gradient-to-r from-cyan-400 via-blue-500 to-indigo-500 transition-all duration-150"
                            style="width: {pct}%"
                          ></div>
                        </div>
                        <div class="flex items-center justify-between text-[10px] font-mono text-slate-400">
                          <span>{pct}% ({formatBytes(transferredBytes)} / {formatBytes(totalBytes)})</span>
                          <span class="text-cyan-400 font-bold">{(linkedTransfer.speed_mbps || 0).toFixed(1)} MB/s</span>
                        </div>
                      </div>
                    {/if}
                  </div>
                {/if}

                <!-- Timestamp & Delivery Indicator (Relative format) -->
                <div class="flex items-center justify-end gap-1.5 mt-1.5 pt-1 text-[10px] font-mono text-slate-500">
                  <span title={new Date(msg.created_at * 1000).toLocaleString()}>
                    <!-- Reactive dependency on relativeTimeTicker -->
                    {#key relativeTimeTicker}
                      {formatRelativeTime(msg.created_at)}
                    {/key}
                  </span>
                  {#if isOutgoing}
                    <CheckCheck class="w-3 h-3 text-cyan-400" />
                  {/if}
                </div>
              </div>
            </div>
          {/each}
        {/if}
      </div>

      <!-- Blocked Peer Warning Banner -->
      {#if isPeerBlocked}
        <div class="p-3 bg-rose-950/60 border-t border-rose-800/80 flex items-center justify-between gap-3 text-xs text-rose-200">
          <div class="flex items-center gap-2">
            <ShieldAlert class="w-4 h-4 text-rose-400 shrink-0" />
            <span>Kontak ini sedang diblokir. Pesan dan transfer file dari kontak ini tidak akan diterima.</span>
          </div>
          <button
            type="button"
            on:click={toggleBlockPeer}
            class="px-3 py-1 rounded-lg bg-rose-500/20 hover:bg-rose-500/30 text-rose-300 border border-rose-500/40 text-xs font-bold transition shrink-0"
          >
            Buka Blokir
          </button>
        </div>
      {/if}

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

      <!-- Emoticon Popover Picker -->
      {#if showEmojiPicker && !isPeerBlocked}
        <div class="p-3 border-t border-slate-800 bg-slate-950/95 max-h-48 overflow-y-auto space-y-2 backdrop-blur-md">
          {#each emojiCategories as cat}
            <div>
              <p class="text-[10px] font-mono text-slate-500 mb-1">{cat.name}</p>
              <div class="flex flex-wrap gap-1">
                {#each cat.emojis as em}
                  <button
                    type="button"
                    on:click={() => insertEmoji(em)}
                    class="w-8 h-8 rounded-lg hover:bg-slate-800 flex items-center justify-center text-lg transition hover:scale-110"
                  >
                    {em}
                  </button>
                {/each}
              </div>
            </div>
          {/each}
        </div>
      {/if}

      <!-- Chat Input Area -->
      <footer class="p-2 sm:p-3.5 border-t border-slate-800/80 bg-slate-950/80 shrink-0">
        <form
          on:submit|preventDefault={handleSendMessage}
          class="flex items-end gap-1.5 sm:gap-2"
        >
          <!-- Attachment Dropdown & Emoji Picker -->
          <div class="flex items-center gap-0.5 sm:gap-1 shrink-0 pb-1">
            <button
              type="button"
              disabled={isPeerBlocked}
              on:click={() => (showEmojiPicker = !showEmojiPicker)}
              class="p-1.5 sm:p-2 rounded-xl text-slate-400 hover:text-amber-300 hover:bg-slate-900 border border-transparent hover:border-slate-800 transition disabled:opacity-40"
              title="Add Emoji"
            >
              <Smile class="w-4 h-4" />
            </button>
            <button
              type="button"
              disabled={isPeerBlocked}
              on:click={handlePickFiles}
              class="p-1.5 sm:p-2 rounded-xl text-slate-400 hover:text-cyan-300 hover:bg-slate-900 border border-transparent hover:border-slate-800 transition disabled:opacity-40"
              title="Attach Multiple Files (Images, Videos, Code, PDFs, Docs, etc.)"
            >
              <Paperclip class="w-4 h-4" />
            </button>
            <button
              type="button"
              disabled={isPeerBlocked}
              on:click={handlePickFolder}
              class="p-1.5 sm:p-2 rounded-xl text-slate-400 hover:text-cyan-300 hover:bg-slate-900 border border-transparent hover:border-slate-800 transition disabled:opacity-40"
              title="Attach Entire Folder"
            >
              <FolderUp class="w-4 h-4" />
            </button>
          </div>

          <!-- Auto-expanding Input Textarea -->
          <div class="flex-1 relative min-w-0">
            <textarea
              bind:this={textareaEl}
              bind:value={messageInput}
              on:input={autoResizeTextarea}
              on:keydown={handleKeyDown}
              disabled={isPeerBlocked}
              rows="1"
              placeholder={isPeerBlocked ? "Kontak ini diblokir" : "Ketik pesan..."}
              class="w-full px-3 sm:px-4 py-2 sm:py-2.5 rounded-xl bg-slate-900 border border-slate-800 text-xs text-white placeholder-slate-500 focus:outline-none focus:border-cyan-500/60 focus:ring-1 focus:ring-cyan-500/30 resize-none max-h-36 transition font-sans disabled:opacity-40"
            ></textarea>
          </div>

          <!-- Send Button -->
          <button
            type="submit"
            disabled={isSending || isPeerBlocked || (!messageInput.trim() && attachingFiles.length === 0)}
            class="px-3 sm:px-4 py-2 sm:py-2.5 rounded-xl bg-gradient-to-r from-cyan-500 to-indigo-600 hover:from-cyan-400 hover:to-indigo-500 disabled:opacity-40 disabled:cursor-not-allowed text-white font-bold text-xs shadow-lg shadow-cyan-500/20 transition-all flex items-center gap-1.5 shrink-0"
          >
            {#if isSending}
              <RefreshCw class="w-3.5 h-3.5 animate-spin" />
            {:else}
              <Send class="w-3.5 h-3.5" />
            {/if}
            <span class="hidden sm:inline">Send</span>
          </button>
        </form>

        <div class="hidden sm:flex items-center justify-between text-[10px] text-slate-500 font-mono mt-2 px-1">
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
  :global(.markdown-body a) {
    color: #38bdf8 !important;
    text-decoration: underline !important;
    cursor: pointer !important;
  }
  :global(.markdown-body a:hover) {
    color: #7dd3fc !important;
  }
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
