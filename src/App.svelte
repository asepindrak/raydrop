<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWindow, UserAttentionType } from "@tauri-apps/api/window";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import Header from "./lib/components/Header.svelte";
  import ContactList from "./lib/components/ContactList.svelte";
  import ActiveTransfers from "./lib/components/ActiveTransfers.svelte";
  import TransferModal from "./lib/components/TransferModal.svelte";
  import TraySettings from "./lib/components/TraySettings.svelte";
  import AuthModal from "./lib/components/AuthModal.svelte";
  import UpdateModal from "./lib/components/UpdateModal.svelte";
  import { Upload } from "lucide-svelte";
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
    isDraggingFiles,
  } from "./lib/stores/state";
  import { contacts } from "./lib/stores/contacts";
  import {
    transfers,
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

  onMount(async () => {
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

    if (!snap.settings.auth_token) {
      authModalOpen.set(true);
    } else {
      await refreshCloudState(
        snap.settings.signaling_url,
        snap.settings.auth_token,
      );
    }

    // Subscribe to real-time backend events
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
  <main class="flex-1 w-full p-4 overflow-hidden relative z-10">
    {#if $activeMainTab === "transfers"}
      <ActiveTransfers />
    {:else}
      <ContactList />
    {/if}
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

  <TransferModal />
  <TraySettings />
  <AuthModal />
  <UpdateModal />
</div>
