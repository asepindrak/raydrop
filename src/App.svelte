<script lang="ts">
  import { onMount } from 'svelte';
  import { getCurrentWindow, UserAttentionType } from '@tauri-apps/api/window';
  import Header from './lib/components/Header.svelte';
  import ContactList from './lib/components/ContactList.svelte';
  import ActiveTransfers from './lib/components/ActiveTransfers.svelte';
  import TransferModal from './lib/components/TransferModal.svelte';
  import TraySettings from './lib/components/TraySettings.svelte';
  import AuthModal from './lib/components/AuthModal.svelte';
  import {
    appSettings,
    deviceCryptoMeta,
    networkInfo,
    cloudMyDevices,
    cloudContacts,
    incomingRequestQueue,
    cliQueuedFiles,
    authModalOpen,
  } from './lib/stores/state';
  import { contacts } from './lib/stores/contacts';
  import {
    transfers,
    applyTransferProgress,
    upsertTransferRecord,
  } from './lib/stores/transfers';
  import {
    raydropApi,
    cloudApi,
    getApiBaseUrl,
    type IncomingTransferEvent,
    type TransferProgressEvent,
    type TransferRecord,
  } from './lib/utils/tauri-ipc';

  async function refreshCloudState(sigUrl: string, token: string | null) {
    if (!token) return;
    try {
      const baseUrl = getApiBaseUrl(sigUrl);
      const cloudState = await cloudApi.getState(baseUrl, token);
      cloudMyDevices.set(cloudState.my_devices);
      cloudContacts.set(cloudState.contacts);
      const synced = await raydropApi.syncCloudDataToLocal(
        cloudState.my_devices,
        cloudState.contacts
      );
      contacts.set(synced);
    } catch (e) {
      console.warn('Cloud state sync failed:', e);
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
    }

    contacts.set(await raydropApi.getContacts());
    transfers.set(await raydropApi.getTransferHistory());

    if (!snap.settings.auth_token) {
      authModalOpen.set(true);
    } else {
      await refreshCloudState(snap.settings.signaling_url, snap.settings.auth_token);
    }

    // Subscribe to real-time backend events
    await raydropApi.listenEvent('peer_discovered', async () => {
      contacts.set(await raydropApi.getContacts());
    });

    await raydropApi.listenEvent('contacts_updated', async () => {
      contacts.set(await raydropApi.getContacts());
    });

    await raydropApi.listenEvent('cloud_state_changed', async () => {
      await refreshCloudState($appSettings.signaling_url, $appSettings.auth_token);
    });

    await raydropApi.listenEvent<boolean>('discoverability_changed', (enabled) => {
      appSettings.update((s) => ({ ...s, discoverable: enabled }));
    });

    await raydropApi.listenEvent<boolean>('autostart_changed', (enabled) => {
      appSettings.update((s) => ({ ...s, autostart: enabled }));
    });

    await raydropApi.listenEvent<boolean>('signaling_status_changed', async (connected) => {
      networkInfo.update((n) => ({ ...n, signalingConnected: connected }));
      if (connected && $appSettings.auth_token) {
        await refreshCloudState($appSettings.signaling_url, $appSettings.auth_token);
      }
    });

    await raydropApi.listenEvent<IncomingTransferEvent>(
      'incoming_transfer_request',
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
          console.warn('Window auto-focus fallback error:', err);
        }
      }
    );

    await raydropApi.listenEvent<TransferProgressEvent>(
      'transfer_progress',
      (evt) => {
        applyTransferProgress(evt);
      }
    );

    await raydropApi.listenEvent<TransferRecord>(
      'transfer_state_changed',
      (rec) => {
        upsertTransferRecord(rec);
      }
    );

    await raydropApi.listenEvent<string[]>('cli_send_files', (paths) => {
      cliQueuedFiles.update((prev) => [...prev, ...paths]);
    });
  });
</script>

<div class="h-screen w-screen flex flex-col bg-slate-950 text-slate-100 select-none">
  <Header />

  <main class="flex-1 grid grid-cols-1 lg:grid-cols-12 gap-4 p-5 overflow-hidden">
    <section class="lg:col-span-7 h-full overflow-hidden">
      <ContactList />
    </section>
    <section class="lg:col-span-5 h-full overflow-hidden">
      <ActiveTransfers />
    </section>
  </main>

  <TransferModal />
  <TraySettings />
  <AuthModal />
</div>
