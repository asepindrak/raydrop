<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { get } from "svelte/store";
  import { getCurrentWindow, UserAttentionType } from "@tauri-apps/api/window";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import Header from "./lib/components/Header.svelte";
  import ContactList from "./lib/components/ContactList.svelte";
  import ActiveTransfers from "./lib/components/ActiveTransfers.svelte";
  import ChatView from "./lib/components/ChatView.svelte";
  import TransferModal from "./lib/components/TransferModal.svelte";
  import TraySettings from "./lib/components/TraySettings.svelte";
  import AuthModal from "./lib/components/AuthModal.svelte";
  import UpdateModal from "./lib/components/UpdateModal.svelte";
  import { Upload, X, Zap, MessageSquare, Wifi, Users } from "lucide-svelte";
  import {
    appSettings,
    deviceCryptoMeta,
    networkInfo,
    cloudMyDevices,
    cloudContacts,
    incomingRequestQueue,
    cliQueuedFiles,
    authModalOpen,
    activeMainTab,
    activeChatPeerId,
    isDraggingFiles,
  } from "./lib/stores/state";
  import {
    fetchUnreadCounts,
    handleIncomingChatMessage,
    markChatRead,
    chatToasts,
    addChatToast,
    removeChatToast,
    totalUnreadCount,
  } from "./lib/stores/chat";
  import { contacts, lanContacts } from "./lib/stores/contacts";
  import {
    transfers,
    activeTransfers,
    applyTransferProgress,
    upsertTransferRecord,
  } from "./lib/stores/transfers";
  import { updateProgress } from "./lib/stores/update";
  import { checkForUpdates } from "./lib/utils/updater";
  import {
    raydropApi,
    cloudApi,
    getApiBaseUrl,
    type IncomingTransferEvent,
    type TransferProgressEvent,
    type TransferRecord,
    type UpdateProgressPayload,
    type ChatMessage,
    type ClipboardSyncedPayload,
  } from "./lib/utils/tauri-ipc";

  async function refreshCloudState(sigUrl: string, token: string | null) {
    if (!token) return;
    try {
      const baseUrl = getApiBaseUrl(sigUrl);
      const cloudState = await cloudApi.getState(baseUrl, token);
      cloudMyDevices.set(cloudState.my_devices);
      cloudContacts.set(cloudState.contacts);
      const synced = await raydropApi.syncCloudDataToLocal(
        cloudState.my_devices,
        cloudState.contacts,
      );
      contacts.set(synced);
    } catch (e) {
      console.warn("Cloud state sync failed:", e);
    }
  }

  let statusSyncTimer: any = null;

  onDestroy(() => {
    if (statusSyncTimer) {
      clearInterval(statusSyncTimer);
    }
  });

  onMount(async () => {
    // 1. Immediately subscribe to all real-time backend events FIRST to prevent startup race conditions
    await raydropApi.listenEvent<boolean>(
      "signaling_status_changed",
      async (connected) => {
        networkInfo.update((n) => ({ ...n, signalingConnected: connected }));
        if (connected && $appSettings.auth_token) {
          await refreshCloudState(
            $appSettings.signaling_url,
            $appSettings.auth_token,
          );
        }
      },
    );

    await raydropApi.listenEvent("peer_discovered", async () => {
      contacts.set(await raydropApi.getContacts());
    });

    await raydropApi.listenEvent("contacts_updated", async () => {
      contacts.set(await raydropApi.getContacts());
    });

    await raydropApi.listenEvent("cloud_state_changed", async () => {
      await refreshCloudState(
        $appSettings.signaling_url,
        $appSettings.auth_token,
      );
    });

    await raydropApi.listenEvent<boolean>(
      "discoverability_changed",
      (enabled) => {
        appSettings.update((s) => ({ ...s, discoverable: enabled }));
      },
    );

    await raydropApi.listenEvent<boolean>("autostart_changed", (enabled) => {
      appSettings.update((s) => ({ ...s, autostart: enabled }));
    });

    // 2. Fetch fresh app state & populate stores
    const snap = await raydropApi.getAppState();
    appSettings.set(snap.settings);
    deviceCryptoMeta.set({
      ed25519Pubkey: snap.ed25519_pubkey,
      osType: snap.os_type,
    });
    networkInfo.set({
      localIp: snap.local_ip,
      listenPort: snap.listen_port,
      wanReflexiveAddr: snap.wan_reflexive_addr,
      signalingConnected: snap.signaling_connected,
    });

    if (snap.queued_cli_files.length > 0) {
      cliQueuedFiles.set(snap.queued_cli_files);
      try {
        const win = getCurrentWindow();
        await win.setAlwaysOnTop(true);
        await win.show();
        await win.unminimize();
        await win.setFocus();
        await win.requestUserAttention(UserAttentionType.Critical);
        setTimeout(() => {
          win.setAlwaysOnTop(false).catch(() => {});
        }, 1800);
      } catch (err) {
        console.warn("Window focus error on init cli:", err);
      }
    }

    contacts.set(await raydropApi.getContacts());
    transfers.set(await raydropApi.getTransferHistory());
    await fetchUnreadCounts();

    if (!snap.settings.auth_token) {
      authModalOpen.set(true);
    } else {
      await refreshCloudState(
        snap.settings.signaling_url,
        snap.settings.auth_token,
      );
    }

    // 3. Periodic synchronization timer (every 4s) to ensure signaling & network status never get stuck
    statusSyncTimer = setInterval(async () => {
      try {
        const fresh = await raydropApi.getAppState();
        networkInfo.update((n) => ({
          ...n,
          signalingConnected: fresh.signaling_connected,
          wanReflexiveAddr: fresh.wan_reflexive_addr || n.wanReflexiveAddr,
        }));
      } catch (_) {}
    }, 4000);

    await raydropApi.listenEvent<IncomingTransferEvent>(
      "incoming_transfer_request",
      async (evt) => {
        incomingRequestQueue.update((q) => [...q, evt]);
        try {
          const win = getCurrentWindow();
          await win.setAlwaysOnTop(true);
          await win.show();
          await win.unminimize();
          await win.setFocus();
          await win.requestUserAttention(UserAttentionType.Critical);
          setTimeout(() => {
            win.setAlwaysOnTop(false).catch(() => {});
          }, 1800);
        } catch (err) {
          console.warn("Window auto-focus fallback error:", err);
        }
      },
    );

    await raydropApi.listenEvent<TransferProgressEvent>(
      "transfer_progress",
      (evt) => {
        applyTransferProgress(evt);
      },
    );

    await raydropApi.listenEvent<TransferRecord>(
      "transfer_state_changed",
      (rec) => {
        upsertTransferRecord(rec);
      },
    );

    await raydropApi.listenEvent<string[]>("cli_send_files", async (paths) => {
      cliQueuedFiles.update((prev) => [...prev, ...paths]);
      try {
        const win = getCurrentWindow();
        await win.setAlwaysOnTop(true);
        await win.show();
        await win.unminimize();
        await win.setFocus();
        await win.requestUserAttention(UserAttentionType.Critical);
        setTimeout(() => {
          win.setAlwaysOnTop(false).catch(() => {});
        }, 1800);
      } catch (err) {
        console.warn("Window focus error on cli send:", err);
      }
    });

    await raydropApi.listenEvent<UpdateProgressPayload>(
      "app_update_progress",
      (payload) => {
        updateProgress.set(payload);
      },
    );

    await raydropApi.listenEvent<ChatMessage>("chat_message_received", async (msg) => {
      handleIncomingChatMessage(msg);
      if ($activeMainTab === "chat" && $activeChatPeerId === msg.peer_device_id) {
        await markChatRead(msg.peer_device_id);
      } else {
        const contactList = get(contacts);
        const peer = contactList.find((c) => c.device_id === msg.peer_device_id);
        const peerName = peer?.alias_name || peer?.user_name || msg.peer_device_id.slice(0, 14);
        addChatToast(msg, peerName);

        try {
          const win = getCurrentWindow();
          await win.requestUserAttention(UserAttentionType.Informational);
        } catch (_) {}
      }
    });

    await raydropApi.listenEvent<ClipboardSyncedPayload>("clipboard_synced", async (payload) => {
      const isImg = payload.content.startsWith("data:image/") || (payload as any).is_image;
      const preview = isImg
        ? "🖼️ [Gambar Clipboard - Siap di-Paste (Ctrl+V)]"
        : payload.content.length > 50
        ? payload.content.slice(0, 50) + "..."
        : payload.content;

      // Ensure system clipboard also receives text if supported on Web / WebView
      if (!isImg && navigator.clipboard && navigator.clipboard.writeText) {
        try {
          await navigator.clipboard.writeText(payload.content);
        } catch (_) {}
      }

      addChatToast(
        {
          id: "clip-" + Date.now(),
          peer_device_id: payload.from_device_id,
          direction: "incoming",
          content: payload.auto_copied
            ? `📋 [Auto-Copied] ${preview}`
            : `📋 [Shared] ${preview}`,
          msg_type: "clipboard",
          is_read: true,
          created_at: Math.floor(Date.now() / 1000),
        },
        payload.sender_alias
      );
    });

    await raydropApi.listenEvent<string>("open_file_requested", (filePath) => {
      addChatToast(
        {
          id: "open-file-" + Date.now(),
          peer_device_id: "local",
          direction: "incoming",
          content: `📁 File berada di: ${filePath}. Buka aplikasi Files / Galeri untuk membukanya.`,
          msg_type: "text",
          is_read: true,
          created_at: Math.floor(Date.now() / 1000),
        },
        "Raydrop Storage"
      );
    });

    await raydropApi.listenEvent<string>("open_folder_requested", (folderPath) => {
      addChatToast(
        {
          id: "open-folder-" + Date.now(),
          peer_device_id: "local",
          direction: "incoming",
          content: `📁 File tersimpan di folder Download (${folderPath}). Buka aplikasi Files / Pengelola File untuk mengaksesnya.`,
          msg_type: "text",
          is_read: true,
          created_at: Math.floor(Date.now() / 1000),
        },
        "Raydrop Storage"
      );
    });

    // Native OS Drag and Drop multiple files/folders listener
    try {
      const webview = getCurrentWebview();
      await webview.onDragDropEvent((event) => {
        if (event.payload.type === "enter" || event.payload.type === "over") {
          isDraggingFiles.set(true);
        } else if (event.payload.type === "drop") {
          isDraggingFiles.set(false);
          const droppedPaths = event.payload.paths;
          if (droppedPaths && droppedPaths.length > 0) {
            cliQueuedFiles.update((prev) => [...prev, ...droppedPaths]);
          }
        } else if (event.payload.type === "leave") {
          isDraggingFiles.set(false);
        }
      });
    } catch (err) {
      console.warn("Native Drag-Drop event setup error:", err);
    }

    // Auto-check for software updates silently on startup after brief initialization
    setTimeout(() => {
      checkForUpdates(false).catch(() => {});
    }, 2500);

    // Periodically re-check for updates every 30 minutes while running in background/tray
    const updateInterval = setInterval(() => {
      checkForUpdates(false).catch(() => {});
    }, 30 * 60 * 1000);

    // Re-check whenever the user unhides or focuses the Raydrop window from tray
    const handleWindowFocus = () => {
      checkForUpdates(false).catch(() => {});
    };
    window.addEventListener("focus", handleWindowFocus);

    return () => {
      clearInterval(updateInterval);
      window.removeEventListener("focus", handleWindowFocus);
    };
  });
</script>

<div
  class="h-screen w-screen flex flex-col bg-slate-950 text-slate-100 select-none overflow-hidden relative"
>
  <!-- Ambient floating blobs (matching landing page aesthetic) -->
  <div
    class="ambient-blob w-[450px] h-[450px] bg-cyan-500/[0.07] -top-24 -left-24"
    aria-hidden="true"
  ></div>
  <div
    class="ambient-blob w-[450px] h-[450px] bg-indigo-500/[0.07] top-1/2 -right-24"
    aria-hidden="true"
  ></div>
  <div
    class="ambient-blob w-[380px] h-[380px] bg-emerald-500/[0.06] -bottom-24 left-1/3"
    aria-hidden="true"
  ></div>

  <Header />

  <!-- Main View: FULL width & height for Transfers & Streams, or active tab -->
  <main class="flex-1 w-full p-2 sm:p-4 pb-16 md:pb-4 overflow-hidden relative z-10 flex flex-col min-h-0">
    <div class="h-full w-full {$activeMainTab === 'transfers' ? 'flex flex-col' : 'hidden'}">
      <ActiveTransfers />
    </div>
    <div class="h-full w-full {$activeMainTab === 'chat' ? 'flex flex-col' : 'hidden'}">
      <ChatView />
    </div>
    <div class="h-full w-full {$activeMainTab !== 'transfers' && $activeMainTab !== 'chat' ? 'flex flex-col' : 'hidden'}">
      <ContactList />
    </div>
  </main>

  <!-- Fullscreen Drag & Drop Overlay Indicator -->
  {#if $isDraggingFiles}
    <div
      class="fixed inset-0 z-50 bg-slate-950/85 backdrop-blur-md flex flex-col items-center justify-center pointer-events-none transition-all"
    >
      <div
        class="p-8 rounded-3xl border-2 border-dashed border-cyan-400 bg-slate-900/95 shadow-2xl shadow-cyan-500/25 flex flex-col items-center text-center max-w-md mx-4 transform scale-105 transition-transform"
      >
        <div
          class="w-20 h-20 rounded-2xl bg-gradient-to-tr from-cyan-500/25 to-indigo-500/25 border border-cyan-400/50 flex items-center justify-center text-cyan-300 mb-4 animate-bounce"
        >
          <Upload class="w-10 h-10 stroke-[2.2]" />
        </div>
        <h3 class="text-xl font-extrabold text-white font-display">
          Drop Files to Transfer
        </h3>
        <p class="text-xs text-slate-300 mt-2">
          Drop any multiple files or folders here to send directly via encrypted
          P2P.
        </p>
        <div
          class="flex items-center gap-2 mt-4 text-[11px] font-mono text-cyan-400 bg-cyan-500/10 px-3 py-1.5 rounded-full border border-cyan-500/30"
        >
          <span>⚡ Unlimited Size • Resumable • Zero Cloud Storage</span>
        </div>
      </div>
    </div>
  {/if}

  <!-- In-app Floating Chat Toast Notifications (Bottom-Right) -->
  {#if $chatToasts.length > 0}
    <div class="fixed bottom-6 right-6 z-50 flex flex-col gap-2.5 max-w-sm pointer-events-auto">
      {#each $chatToasts as toast (toast.id)}
        <div
          role="button"
          tabindex="0"
          on:click={() => {
            activeMainTab.set("chat");
            activeChatPeerId.set(toast.peerDeviceId);
            removeChatToast(toast.id);
          }}
          on:keydown={(e) => {
            if (e.key === "Enter") {
              activeMainTab.set("chat");
              activeChatPeerId.set(toast.peerDeviceId);
              removeChatToast(toast.id);
            }
          }}
          class="p-4 rounded-2xl bg-slate-900/95 border border-cyan-500/50 shadow-2xl shadow-cyan-500/20 backdrop-blur-xl flex items-start justify-between gap-3 cursor-pointer hover:border-cyan-400 hover:scale-[1.02] transition-all transform animate-in slide-in-from-bottom duration-200"
        >
          <div class="flex items-start gap-3 min-w-0">
            <div class="w-9 h-9 rounded-xl bg-gradient-to-tr from-cyan-500/20 to-indigo-500/20 border border-cyan-500/40 flex items-center justify-center text-cyan-300 font-bold text-xs shrink-0 mt-0.5">
              {toast.peerName.slice(0, 2).toUpperCase()}
            </div>
            <div class="min-w-0">
              <div class="flex items-center gap-1.5">
                <span class="text-xs font-bold text-white truncate">{toast.peerName}</span>
                <span class="text-[10px] text-cyan-400 font-mono">Pesan Baru</span>
              </div>
              <p class="text-xs text-slate-300 truncate mt-0.5 max-w-[220px]">
                {toast.content}
              </p>
              <span class="text-[10px] text-cyan-400/80 font-mono mt-1 block">
                Klik untuk buka chat &rarr;
              </span>
            </div>
          </div>
          <button
            type="button"
            on:click|stopPropagation={() => removeChatToast(toast.id)}
            class="text-slate-400 hover:text-white p-1 rounded-lg hover:bg-slate-800 transition shrink-0"
          >
            <X class="w-3.5 h-3.5" />
          </button>
        </div>
      {/each}
    </div>
  {/if}

  <TransferModal />
  <TraySettings />
  <AuthModal />
  <UpdateModal />

  <!-- Mobile Bottom Navigation Bar (md:hidden) -->
  <nav class="md:hidden fixed bottom-0 left-0 right-0 z-40 bg-slate-950/95 border-t border-slate-800/90 backdrop-blur-xl px-2 py-1.5 flex items-center justify-around shadow-2xl">
    <!-- 1. Transfers -->
    <button
      type="button"
      on:click={() => activeMainTab.set('transfers')}
      class="flex flex-col items-center gap-0.5 px-3 py-1 rounded-xl transition-all {$activeMainTab === 'transfers'
        ? 'text-cyan-400 font-bold'
        : 'text-slate-400 hover:text-slate-200'}"
    >
      <div class="relative">
        <Zap class="w-5 h-5" />
        {#if $activeTransfers.length > 0}
          <span class="absolute -top-1 -right-2 w-3.5 h-3.5 rounded-full bg-cyan-400 text-slate-950 text-[9px] font-bold flex items-center justify-center animate-pulse">
            {$activeTransfers.length}
          </span>
        {/if}
      </div>
      <span class="text-[10px]">Transfers</span>
    </button>

    <!-- 2. Chat -->
    <button
      type="button"
      on:click={() => activeMainTab.set('chat')}
      class="flex flex-col items-center gap-0.5 px-3 py-1 rounded-xl transition-all {$activeMainTab === 'chat'
        ? 'text-cyan-400 font-bold'
        : 'text-slate-400 hover:text-slate-200'}"
    >
      <div class="relative">
        <MessageSquare class="w-5 h-5" />
        {#if $totalUnreadCount > 0}
          <span class="absolute -top-1 -right-2 w-3.5 h-3.5 rounded-full bg-rose-500 text-white text-[9px] font-bold flex items-center justify-center animate-pulse">
            {$totalUnreadCount}
          </span>
        {/if}
      </div>
      <span class="text-[10px]">Chat</span>
    </button>

    <!-- 3. LAN Radar -->
    <button
      type="button"
      on:click={() => activeMainTab.set('lan')}
      class="flex flex-col items-center gap-0.5 px-3 py-1 rounded-xl transition-all {$activeMainTab === 'lan'
        ? 'text-cyan-400 font-bold'
        : 'text-slate-400 hover:text-slate-200'}"
    >
      <div class="relative">
        <Wifi class="w-5 h-5" />
        {#if $lanContacts.length > 0}
          <span class="absolute -top-1 -right-2 w-2 h-2 rounded-full bg-emerald-400 animate-pulse"></span>
        {/if}
      </div>
      <span class="text-[10px]">Radar</span>
    </button>

    <!-- 4. Contacts -->
    <button
      type="button"
      on:click={() => activeMainTab.set('contacts')}
      class="flex flex-col items-center gap-0.5 px-3 py-1 rounded-xl transition-all {$activeMainTab === 'contacts'
        ? 'text-cyan-400 font-bold'
        : 'text-slate-400 hover:text-slate-200'}"
    >
      <Users class="w-5 h-5" />
      <span class="text-[10px]">Peers</span>
    </button>
  </nav>
</div>
