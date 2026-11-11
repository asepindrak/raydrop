<script lang="ts">
  import { onMount } from 'svelte';
  import Header from './lib/components/Header.svelte';
  import ContactList from './lib/components/ContactList.svelte';
  import ActiveTransfers from './lib/components/ActiveTransfers.svelte';
  import TransferModal from './lib/components/TransferModal.svelte';
  import TraySettings from './lib/components/TraySettings.svelte';
  import {
    appSettings,
    networkInfo,
    incomingRequestQueue,
    cliQueuedFiles,
  } from './lib/stores/state';
  import { contacts } from './lib/stores/contacts';
  import {
    transfers,
    applyTransferProgress,
    upsertTransferRecord,
  } from './lib/stores/transfers';
  import {
    raydropApi,
    type IncomingTransferEvent,
    type TransferProgressEvent,
    type TransferRecord,
  } from './lib/utils/tauri-ipc';

  onMount(async () => {
    const snap = await raydropApi.getAppState();
    appSettings.set(snap.settings);
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

    // Subscribe to real-time backend events
    await raydropApi.listenEvent('peer_discovered', async () => {
      contacts.set(await raydropApi.getContacts());
    });

    await raydropApi.listenEvent('contacts_updated', async () => {
      contacts.set(await raydropApi.getContacts());
    });

    await raydropApi.listenEvent<boolean>('discoverability_changed', (enabled) => {
      appSettings.update((s) => ({ ...s, discoverable: enabled }));
    });

    await raydropApi.listenEvent<boolean>('signaling_status_changed', (connected) => {
      networkInfo.update((n) => ({ ...n, signalingConnected: connected }));
    });

    await raydropApi.listenEvent<IncomingTransferEvent>(
      'incoming_transfer_request',
      (evt) => {
        incomingRequestQueue.update((q) => [...q, evt]);
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
</div>

