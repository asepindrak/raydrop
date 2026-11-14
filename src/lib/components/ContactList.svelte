<script lang="ts">
  import { open } from '@tauri-apps/plugin-dialog';
  import {
    appSettings,
    cloudMyDevices,
    cloudContacts,
    authModalOpen,
    activeMainTab,
    activeChatPeerId,
  } from '../stores/state';
  import { contacts, lanContacts } from '../stores/contacts';
  import {
    raydropApi,
    cloudApi,
    getApiBaseUrl,
    type Contact,
    type CloudDeviceNode,
    type CloudContactGroup,
  } from '../utils/tauri-ipc';
  import {
    Wifi,
    UserPlus,
    Send,
    FolderUp,
    Laptop,
    Users,
    Pencil,
    Trash2,
    Check,
    X,
    Mail,
    LogIn,
    ShieldCheck,
    ArrowLeft,
    MessageSquare,
  } from 'lucide-svelte';

  function openChatWith(deviceId: string) {
    activeChatPeerId.set(deviceId);
    activeMainTab.set('chat');
  }

  let activeTab: 'devices' | 'contacts' | 'lan' = 'devices';

  $: if ($activeMainTab === 'lan' || $activeMainTab === 'devices' || $activeMainTab === 'contacts') {
    activeTab = $activeMainTab;
  }

  function selectTab(tab: 'devices' | 'contacts' | 'lan') {
    activeTab = tab;
    activeMainTab.set(tab);
  }
  let showAddContactModal = false;
  let newContactEmail = '';
  let errorMsg = '';

  let editingDeviceId: string | null = null;
  let editingDeviceName = '';

  async function syncWithCloud() {
    if (!$appSettings.auth_token) return;
    try {
      const baseUrl = getApiBaseUrl($appSettings.signaling_url);
      const state = await cloudApi.getState(baseUrl, $appSettings.auth_token);
      cloudMyDevices.set(state.my_devices);
      cloudContacts.set(state.contacts);
      const localList = await raydropApi.syncCloudDataToLocal(
        state.my_devices,
        state.contacts
      );
      contacts.set(localList);
    } catch (e) {
      console.warn('Cloud sync warning:', e);
    }
  }

  async function handleAddContactByEmail() {
    errorMsg = '';
    if (!newContactEmail.trim() || !newContactEmail.includes('@')) {
      errorMsg = 'Please enter a valid registered email address.';
      return;
    }
    if (!$appSettings.auth_token) {
      authModalOpen.set(true);
      return;
    }
    try {
      const baseUrl = getApiBaseUrl($appSettings.signaling_url);
      const res = await cloudApi.addContactByEmail(
        baseUrl,
        $appSettings.auth_token,
        newContactEmail.trim()
      );
      cloudContacts.set(res.contacts);
      const localList = await raydropApi.syncCloudDataToLocal(
        $cloudMyDevices,
        res.contacts
      );
      contacts.set(localList);
      newContactEmail = '';
      showAddContactModal = false;
    } catch (e: any) {
      errorMsg = e?.message || String(e);
    }
  }

  async function handleRenameDevice(deviceId: string) {
    if (!$appSettings.auth_token || !editingDeviceName.trim()) return;
    try {
      const baseUrl = getApiBaseUrl($appSettings.signaling_url);
      const res = await cloudApi.renameDevice(
        baseUrl,
        $appSettings.auth_token,
        deviceId,
        editingDeviceName.trim()
      );
      cloudMyDevices.set(res.my_devices);
      if (deviceId === $appSettings.device_id) {
        appSettings.update((s) => ({ ...s, alias_name: editingDeviceName.trim() }));
      }
      const localList = await raydropApi.syncCloudDataToLocal(
        res.my_devices,
        $cloudContacts
      );
      contacts.set(localList);
      editingDeviceId = null;
    } catch (e) {
      console.error(e);
    }
  }

  async function handleRemoveDevice(deviceId: string) {
    if (!$appSettings.auth_token) return;
    try {
      const baseUrl = getApiBaseUrl($appSettings.signaling_url);
      const res = await cloudApi.deleteDevice(
        baseUrl,
        $appSettings.auth_token,
        deviceId
      );
      cloudMyDevices.set(res.my_devices);
      await raydropApi.deleteLocalContact({ deviceId });
      contacts.set(await raydropApi.getContacts());
    } catch (e) {
      console.error(e);
    }
  }

  async function handleContactTrustChange(email: string, nextTrust: string) {
    if (!$appSettings.auth_token) return;
    try {
      const baseUrl = getApiBaseUrl($appSettings.signaling_url);
      const res = await cloudApi.updateContactTrust(
        baseUrl,
        $appSettings.auth_token,
        email,
        nextTrust
      );
      cloudContacts.set(res.contacts);
      await raydropApi.updateContactTrust(
        '',
        nextTrust as Contact['trust_level'],
        email
      );
      contacts.set(await raydropApi.getContacts());
    } catch (e) {
      console.error(e);
    }
  }

  async function handleRemoveContact(email: string) {
    if (!$appSettings.auth_token) return;
    try {
      const baseUrl = getApiBaseUrl($appSettings.signaling_url);
      const res = await cloudApi.deleteContactByEmail(
        baseUrl,
        $appSettings.auth_token,
        email
      );
      cloudContacts.set(res.contacts);
      await raydropApi.deleteLocalContact({ email });
      contacts.set(await raydropApi.getContacts());
    } catch (e) {
      console.error(e);
    }
  }

  async function handleLanTrustChange(peer: Contact, nextTrust: string) {
    await raydropApi.updateContactTrust(
      peer.device_id,
      nextTrust as Contact['trust_level'],
      peer.user_email
    );
    contacts.set(await raydropApi.getContacts());
  }

  async function sendToDeviceId(deviceId: string, label: string, directory = false) {
    try {
      const isMobile = /Android|iPhone|iPad/i.test(navigator.userAgent);
      const shouldPickDir = directory && !isMobile;
      const selected = await open({
        multiple: true,
        directory: shouldPickDir,
        title: shouldPickDir ? `Send Folder to ${label}` : `Send Files to ${label}`,
      });
      if (!selected) return;
      const paths = Array.isArray(selected) ? selected : [selected];
      if (paths.length > 0) {
        await raydropApi.initiateTransfer(deviceId, paths);
        activeMainTab.set('transfers');
      }
    } catch (err: any) {
      console.error('Failed to send files:', err);
      const msg = err?.toString() || '';
      if (msg.includes('FolderPickerNotImplemented')) {
        alert('Android tidak mendukung pemilihan folder langsung. Silakan gunakan tombol Kirim File.');
      } else {
        alert('Gagal mengirim file: ' + (msg || 'Unknown error'));
      }
    }
  }

  function pickPrimaryDevice(group: CloudContactGroup): CloudDeviceNode | null {
    return (
      group.devices.find((d) => d.is_online) || group.devices[0] || null
    );
  }
</script>

<div class="flex flex-col h-full bg-slate-900/40 border border-slate-800/80 rounded-2xl overflow-hidden">
  <!-- Navigation Tabs -->
  <div class="p-3 sm:p-4 border-b border-slate-800/80 flex flex-wrap items-center justify-between gap-2 sm:gap-3 bg-slate-900/60 backdrop-blur-xl">
    <div class="flex items-center gap-1.5 sm:gap-2 flex-wrap">
      <button
        type="button"
        on:click={() => activeMainTab.set('transfers')}
        class="hidden sm:flex items-center gap-1.5 px-3 py-1.5 rounded-xl bg-slate-800/80 hover:bg-slate-700 text-slate-300 hover:text-white text-xs font-semibold border border-slate-700 transition-colors"
        title="Return to Transfers & Streams"
      >
        <ArrowLeft class="w-3.5 h-3.5 text-cyan-400" />
        <span>Transfers</span>
      </button>

      <div class="flex items-center gap-1 bg-slate-950/70 p-1 rounded-xl border border-slate-800 overflow-x-auto max-w-full">
        <button
          on:click={() => selectTab('devices')}
          class="flex items-center gap-1.5 px-2.5 sm:px-3 py-1.5 rounded-lg text-xs font-semibold transition-all shrink-0 {activeTab ===
          'devices'
            ? 'bg-cyan-500/20 text-cyan-300 border border-cyan-500/30'
            : 'text-slate-400 hover:text-slate-200'}"
        >
          <Laptop class="w-3.5 h-3.5" />
          <span>My Devices ({$cloudMyDevices.length})</span>
        </button>

        <button
          on:click={() => selectTab('contacts')}
          class="flex items-center gap-1.5 px-2.5 sm:px-3 py-1.5 rounded-lg text-xs font-semibold transition-all shrink-0 {activeTab ===
          'contacts'
            ? 'bg-cyan-500/20 text-cyan-300 border border-cyan-500/30'
            : 'text-slate-400 hover:text-slate-200'}"
        >
          <Users class="w-3.5 h-3.5" />
          <span>Contacts ({$cloudContacts.length})</span>
        </button>

        <button
          on:click={() => selectTab('lan')}
          class="flex items-center gap-1.5 px-2.5 sm:px-3 py-1.5 rounded-lg text-xs font-semibold transition-all shrink-0 {activeTab ===
          'lan'
            ? 'bg-cyan-500/20 text-cyan-300 border border-cyan-500/30'
            : 'text-slate-400 hover:text-slate-200'}"
        >
          <Wifi class="w-3.5 h-3.5" />
          <span>LAN Radar ({$lanContacts.length})</span>
        </button>
      </div>
    </div>

    {#if $appSettings.auth_token}
      <button
        on:click={() => {
          errorMsg = '';
          showAddContactModal = true;
        }}
        class="flex items-center gap-1.5 px-3 py-1.5 rounded-xl bg-cyan-500 hover:bg-cyan-400 text-slate-950 font-bold text-xs transition-colors shadow-sm shadow-cyan-500/20 shrink-0"
      >
        <UserPlus class="w-3.5 h-3.5" />
        <span class="hidden sm:inline">Add by Email</span>
        <span class="sm:hidden">Add</span>
      </button>
    {/if}
  </div>

  <!-- Tab Contents -->
  <div class="flex-1 overflow-y-auto p-4 space-y-2.5">
    {#if activeTab === 'devices'}
      {#if !$appSettings.auth_token}
        <div class="h-60 flex flex-col items-center justify-center text-center px-6">
          <div class="w-12 h-12 rounded-2xl bg-cyan-500/10 border border-cyan-500/30 flex items-center justify-center mb-3 text-cyan-400">
            <Laptop class="w-6 h-6" />
          </div>
          <p class="text-sm font-bold text-white">Sign in to manage your devices like Tailscale</p>
          <p class="text-xs text-slate-400 mt-1 max-w-sm">
            Link all your PCs and laptops under one email address for zero-prompt auto-accepted P2P transfers and persistent contacts.
          </p>
          <button
            on:click={() => authModalOpen.set(true)}
            class="mt-4 flex items-center gap-2 px-4 py-2 rounded-xl bg-cyan-500 hover:bg-cyan-400 text-slate-950 font-bold text-xs shadow-lg shadow-cyan-500/20"
          >
            <LogIn class="w-4 h-4" />
            <span>Sign In / Register Email</span>
          </button>
        </div>
      {:else}
        {#each $cloudMyDevices as dev (dev.device_id)}
          {@const isThisDevice = dev.device_id === $appSettings.device_id}
          <div class="p-3.5 rounded-xl bg-slate-900/90 border {isThisDevice ? 'border-cyan-500/40' : 'border-slate-800'} flex flex-col sm:flex-row sm:items-center justify-between gap-3">
            <div class="flex items-center gap-3 min-w-0">
              <div class="relative w-10 h-10 rounded-xl bg-slate-800 border border-slate-700 flex items-center justify-center shrink-0">
                <Laptop class="w-5 h-5 text-cyan-400" />
                <span
                  class="absolute -bottom-0.5 -right-0.5 w-2.5 h-2.5 rounded-full ring-2 ring-slate-900 {dev.is_online || isThisDevice
                    ? 'bg-emerald-400'
                    : 'bg-slate-500'}"
                ></span>
              </div>
              <div class="min-w-0">
                {#if editingDeviceId === dev.device_id}
                  <div class="flex items-center gap-1.5">
                    <input
                      type="text"
                      bind:value={editingDeviceName}
                      class="px-2 py-0.5 rounded bg-slate-950 border border-cyan-500 text-xs text-white focus:outline-none"
                    />
                    <button
                      on:click={() => handleRenameDevice(dev.device_id)}
                      class="p-1 text-emerald-400 hover:bg-slate-800 rounded"
                    >
                      <Check class="w-3.5 h-3.5" />
                    </button>
                    <button
                      on:click={() => (editingDeviceId = null)}
                      class="p-1 text-slate-400 hover:bg-slate-800 rounded"
                    >
                      <X class="w-3.5 h-3.5" />
                    </button>
                  </div>
                {:else}
                  <div class="flex items-center gap-2 flex-wrap">
                    <p class="text-sm font-semibold text-slate-100 truncate">{dev.device_name}</p>
                    {#if isThisDevice}
                      <span class="text-[10px] font-bold px-2 py-0.5 rounded-full bg-cyan-500/15 text-cyan-300 border border-cyan-500/30">
                        This Device
                      </span>
                    {:else}
                      <span class="text-[10px] font-medium px-1.5 py-0.5 rounded bg-emerald-500/10 text-emerald-300 border border-emerald-500/20 flex items-center gap-1">
                        <ShieldCheck class="w-3 h-3" /> Auto-Accept
                      </span>
                    {/if}
                    <span class="text-[10px] uppercase font-mono px-1.5 py-0.5 rounded bg-slate-800 text-slate-400 border border-slate-700">
                      {dev.os_type}
                    </span>
                  </div>
                {/if}
                <p class="text-xs font-mono text-slate-400 truncate mt-0.5">
                  {dev.device_id}
                  {#if dev.last_ip}
                    <span class="text-slate-500">• {dev.last_ip}</span>
                  {/if}
                </p>
              </div>
            </div>

            <div class="flex items-center gap-1.5 flex-wrap w-full sm:w-auto justify-end pt-2 sm:pt-0 border-t sm:border-t-0 border-slate-800/60">
              <button
                on:click={() => {
                  editingDeviceId = dev.device_id;
                  editingDeviceName = dev.device_name;
                }}
                class="p-1.5 rounded-lg bg-slate-800/80 hover:bg-slate-700 text-slate-300 border border-slate-700"
                title="Rename Device"
              >
                <Pencil class="w-3.5 h-3.5" />
              </button>

              {#if !isThisDevice}
                <button
                  on:click={() => openChatWith(dev.device_id)}
                  class="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-indigo-500/15 hover:bg-indigo-500 text-indigo-300 hover:text-slate-950 border border-indigo-500/30 text-xs font-semibold transition-all"
                  title="P2P Chat"
                >
                  <MessageSquare class="w-3.5 h-3.5" />
                  <span>Chat</span>
                </button>
                <button
                  on:click={() => sendToDeviceId(dev.device_id, dev.device_name, false)}
                  class="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-cyan-500/15 hover:bg-cyan-500 text-cyan-300 hover:text-slate-950 border border-cyan-500/30 text-xs font-semibold transition-all"
                >
                  <Send class="w-3.5 h-3.5" />
                  <span>Files</span>
                </button>
                <button
                  on:click={() => sendToDeviceId(dev.device_id, dev.device_name, true)}
                  class="p-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-300 border border-slate-700"
                  title="Send Folder"
                >
                  <FolderUp class="w-4 h-4" />
                </button>
                <button
                  on:click={() => handleRemoveDevice(dev.device_id)}
                  class="p-1.5 rounded-lg bg-rose-500/10 hover:bg-rose-500/20 text-rose-400 border border-rose-500/20"
                  title="Remove Device from Account"
                >
                  <Trash2 class="w-3.5 h-3.5" />
                </button>
              {/if}
            </div>
          </div>
        {/each}
      {/if}

    {:else if activeTab === 'contacts'}
      {#if !$appSettings.auth_token}
        <div class="h-60 flex flex-col items-center justify-center text-center px-6">
          <p class="text-sm font-semibold text-slate-200">Sign in to sync your persistent Email contacts</p>
          <button
            on:click={() => authModalOpen.set(true)}
            class="mt-3 px-4 py-2 rounded-xl bg-cyan-500 hover:bg-cyan-400 text-slate-950 font-bold text-xs"
          >
            Sign In
          </button>
        </div>
      {:else}
        {#each $cloudContacts as group (group.email)}
          {@const primaryDev = pickPrimaryDevice(group)}
          <div class="p-3.5 rounded-xl bg-slate-900/90 border border-slate-800 space-y-2.5">
            <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-3">
              <div class="flex items-center gap-3 min-w-0">
                <div class="relative w-10 h-10 rounded-xl bg-slate-800 border border-slate-700 flex items-center justify-center shrink-0">
                  <Mail class="w-5 h-5 text-cyan-400" />
                  <span
                    class="absolute -bottom-0.5 -right-0.5 w-2.5 h-2.5 rounded-full ring-2 ring-slate-900 {group.is_online
                      ? 'bg-emerald-400'
                      : 'bg-slate-500'}"
                  ></span>
                </div>
                <div class="min-w-0">
                  <div class="flex items-center gap-2 flex-wrap">
                    <p class="text-sm font-bold text-white truncate">{group.name}</p>
                    <span class="text-[10px] font-mono px-2 py-0.5 rounded-full bg-slate-800 text-cyan-300 border border-slate-700">
                      {group.devices.length} device(s)
                    </span>
                  </div>
                  <p class="text-xs font-mono text-slate-400 truncate">{group.email}</p>
                </div>
              </div>

              <div class="flex items-center gap-1.5 flex-wrap w-full sm:w-auto justify-between sm:justify-end pt-2 sm:pt-0 border-t sm:border-t-0 border-slate-800/60">
                <select
                  value={group.trust_level}
                  on:change={(e) => handleContactTrustChange(group.email, e.currentTarget.value)}
                  class="flex-1 sm:flex-initial bg-slate-950 border border-slate-700/80 rounded-lg px-2 py-1.5 text-[11px] text-slate-300 focus:outline-none focus:border-cyan-500"
                >
                  <option value="ask">Ask</option>
                  <option value="trusted_auto_accept">Auto-Accept</option>
                  <option value="blocked">Blocked</option>
                </select>

                {#if primaryDev}
                  <div class="flex items-center gap-1.5 shrink-0">
                    <button
                      on:click={() => openChatWith(primaryDev.device_id)}
                      class="flex items-center gap-1.5 px-2.5 sm:px-3 py-1.5 rounded-lg bg-indigo-500/15 hover:bg-indigo-500 text-indigo-300 hover:text-slate-950 border border-indigo-500/30 text-xs font-semibold transition-all"
                      title="P2P Chat"
                    >
                      <MessageSquare class="w-3.5 h-3.5" />
                      <span>Chat</span>
                    </button>
                    <button
                      on:click={() => sendToDeviceId(primaryDev.device_id, `${group.name} (${primaryDev.device_name})`, false)}
                      class="flex items-center gap-1.5 px-2.5 sm:px-3 py-1.5 rounded-lg bg-cyan-500/15 hover:bg-cyan-500 text-cyan-300 hover:text-slate-950 border border-cyan-500/30 text-xs font-semibold transition-all"
                    >
                      <Send class="w-3.5 h-3.5" />
                      <span>Send</span>
                    </button>
                    <button
                      on:click={() => sendToDeviceId(primaryDev.device_id, `${group.name} (${primaryDev.device_name})`, true)}
                      class="p-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-300 border border-slate-700"
                      title="Send Folder"
                    >
                      <FolderUp class="w-4 h-4" />
                    </button>
                  </div>
                {/if}

                <button
                  on:click={() => handleRemoveContact(group.email)}
                  class="p-1.5 rounded-lg bg-rose-500/10 hover:bg-rose-500/20 text-rose-400 border border-rose-500/20 shrink-0"
                  title="Remove Contact"
                >
                  <Trash2 class="w-3.5 h-3.5" />
                </button>
              </div>
            </div>

            <!-- List of Contact's Devices if > 0 -->
            {#if group.devices.length > 0}
              <div class="pt-2 border-t border-slate-800/70 flex flex-wrap gap-1.5">
                {#each group.devices as cdev (cdev.device_id)}
                  <button
                    on:click={() => sendToDeviceId(cdev.device_id, `${group.name} - ${cdev.device_name}`, false)}
                    class="flex items-center gap-1.5 px-2.5 py-1 rounded-lg bg-slate-950 hover:bg-slate-800 border border-slate-800 text-[11px] text-slate-300 transition-colors"
                    title="Click to send files directly to {cdev.device_name}"
                  >
                    <span class="w-1.5 h-1.5 rounded-full {cdev.is_online ? 'bg-emerald-400' : 'bg-slate-600'}"></span>
                    <span class="font-medium">{cdev.device_name}</span>
                    <span class="text-[10px] text-slate-500 uppercase">({cdev.os_type})</span>
                  </button>
                {/each}
              </div>
            {/if}
          </div>
        {:else}
          <div class="h-56 flex flex-col items-center justify-center text-center px-6">
            <Users class="w-8 h-8 text-cyan-400/60 mb-2" />
            <p class="text-sm font-medium text-slate-300">No contacts added yet</p>
            <p class="text-xs text-slate-500 mt-1 max-w-xs">
              Click <strong>Add by Email</strong> to add a friend or colleague by their registered email address. Contacts are saved persistently on the server.
            </p>
          </div>
        {/each}
      {/if}

    {:else}
      <!-- LAN Radar Tab -->
      {#each $lanContacts as peer (peer.device_id)}
        <div class="p-3.5 rounded-xl bg-slate-900/90 border border-slate-800 flex flex-col sm:flex-row sm:items-center justify-between gap-3">
          <div class="flex items-center gap-3 min-w-0">
            <div class="relative w-10 h-10 rounded-xl bg-slate-800 border border-slate-700 flex items-center justify-center shrink-0">
              <Laptop class="w-5 h-5 text-cyan-400" />
              <span class="absolute -bottom-0.5 -right-0.5 w-2.5 h-2.5 rounded-full ring-2 ring-slate-900 bg-emerald-400"></span>
            </div>
            <div class="min-w-0">
              <div class="flex items-center gap-2 flex-wrap">
                <p class="text-sm font-semibold text-slate-100 truncate">{peer.alias_name}</p>
                {#if peer.user_email}
                  <span class="text-[11px] font-mono text-cyan-300 truncate">{peer.user_email}</span>
                {/if}
              </div>
              <p class="text-xs font-mono text-slate-400 truncate mt-0.5">
                {peer.device_id}
                {#if peer.last_known_ip}
                  <span class="text-slate-500">• {peer.last_known_ip}</span>
                {/if}
              </p>
            </div>
          </div>

          <div class="flex items-center gap-1.5 flex-wrap w-full sm:w-auto justify-between sm:justify-end pt-2 sm:pt-0 border-t sm:border-t-0 border-slate-800/60">
            <select
              value={peer.trust_level}
              on:change={(e) => handleLanTrustChange(peer, e.currentTarget.value)}
              class="flex-1 sm:flex-initial bg-slate-950 border border-slate-700/80 rounded-lg px-2 py-1.5 text-[11px] text-slate-300 focus:outline-none focus:border-cyan-500"
            >
              <option value="ask">Ask</option>
              <option value="trusted_auto_accept">Auto-Accept</option>
              <option value="blocked">Blocked</option>
            </select>

            <div class="flex items-center gap-1.5 shrink-0">
              <button
                on:click={() => openChatWith(peer.device_id)}
                class="flex items-center gap-1.5 px-2.5 sm:px-3 py-1.5 rounded-lg bg-indigo-500/15 hover:bg-indigo-500 text-indigo-300 hover:text-slate-950 border border-indigo-500/30 text-xs font-semibold transition-all"
                title="P2P Chat"
              >
                <MessageSquare class="w-3.5 h-3.5" />
                <span>Chat</span>
              </button>
              <button
                on:click={() => sendToDeviceId(peer.device_id, peer.alias_name, false)}
                class="flex items-center gap-1.5 px-2.5 sm:px-3 py-1.5 rounded-lg bg-cyan-500/15 hover:bg-cyan-500 text-cyan-300 hover:text-slate-950 border border-cyan-500/30 text-xs font-semibold transition-all"
              >
                <Send class="w-3.5 h-3.5" />
                <span>Files</span>
              </button>
              <button
                on:click={() => sendToDeviceId(peer.device_id, peer.alias_name, true)}
                class="p-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-300 border border-slate-700"
                title="Send Folder"
              >
                <FolderUp class="w-4 h-4" />
              </button>
            </div>
          </div>
        </div>
      {:else}
        <div class="h-56 flex flex-col items-center justify-center text-center px-6">
          <Wifi class="w-8 h-8 text-cyan-400/70 animate-pulse mb-2" />
          <p class="text-sm font-medium text-slate-300">Scanning local Wi-Fi / LAN via mDNS...</p>
          <p class="text-xs text-slate-500 mt-1 max-w-xs">
            Any device running Raydrop on the same local network will appear here automatically.
          </p>
        </div>
      {/each}
    {/if}
  </div>
</div>

<!-- Add Contact by Email Modal -->
{#if showAddContactModal}
  <div class="fixed inset-0 z-50 bg-slate-950/80 backdrop-blur-sm flex items-center justify-center p-4">
    <div class="w-full max-w-md bg-slate-900 border border-slate-800 rounded-2xl p-6 shadow-2xl">
      <h3 class="text-base font-bold text-white">Add Contact by Email</h3>
      <p class="text-xs text-slate-400 mt-1">
        Enter the email address of a registered Raydrop user. Their devices and online status will sync automatically and persist in your account.
      </p>

      <div class="mt-4">
        <label for="contact-email-input" class="block text-xs font-medium text-slate-300 mb-1">
          Peer Email Address
        </label>
        <input
          id="contact-email-input"
          type="email"
          bind:value={newContactEmail}
          placeholder="friend@example.com"
          class="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-sm text-white focus:outline-none focus:border-cyan-500"
        />
        {#if errorMsg}
          <p class="text-xs text-rose-400 mt-2">{errorMsg}</p>
        {/if}
      </div>

      <div class="flex items-center justify-end gap-2 mt-6">
        <button
          on:click={() => (showAddContactModal = false)}
          class="px-4 py-2 rounded-xl bg-slate-800 hover:bg-slate-700 text-xs font-semibold text-slate-300"
        >
          Cancel
        </button>
        <button
          on:click={handleAddContactByEmail}
          class="px-4 py-2 rounded-xl bg-cyan-500 hover:bg-cyan-400 text-xs font-bold text-slate-950"
        >
          Add Contact
        </button>
      </div>
    </div>
  </div>
{/if}
