import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export interface AppSettings {
  device_id: string;
  alias_name: string;
  user_id: string | null;
  user_email: string | null;
  user_name: string | null;
  auth_token: string | null;
  discoverable: boolean;
  autostart: boolean;
  download_dir: string;
  signaling_url: string;
  stun_addr: string;
}

export interface AppStateSnapshot {
  settings: AppSettings;
  ed25519_pubkey: string;
  os_type: string;
  local_ip: string;
  listen_port: number;
  wan_reflexive_addr: string | null;
  signaling_connected: boolean;
  queued_cli_files: string[];
}

export interface Contact {
  device_id: string;
  alias_name: string;
  user_email: string | null;
  user_name: string | null;
  os_type: string | null;
  is_own_device: boolean;
  ed25519_pubkey: string | null;
  network_type: 'lan' | 'wan';
  last_known_ip: string | null;
  last_known_port: number | null;
  trust_level: 'ask' | 'trusted_auto_accept' | 'blocked';
  is_online: boolean;
  last_seen_at: number;
  created_at: number;
}

export interface CloudDeviceNode {
  device_id: string;
  user_id: string;
  user_email: string;
  user_name: string;
  device_name: string;
  os_type: string;
  ed25519_pubkey: string;
  last_ip: string | null;
  is_online: boolean;
  last_seen_at: number;
  created_at: number;
}

export interface CloudContactGroup {
  user_id: string;
  email: string;
  name: string;
  trust_level: 'ask' | 'trusted_auto_accept' | 'blocked';
  is_online: boolean;
  devices: CloudDeviceNode[];
}

export interface CloudStateResponse {
  token?: string;
  user: {
    id: string;
    email: string;
    name: string;
  };
  my_devices: CloudDeviceNode[];
  contacts: CloudContactGroup[];
}

export interface ManifestFileEntry {
  file_id: number;
  relative_path: string;
  size_bytes: number;
  modified_unix_ms: number;
}

export interface IncomingTransferEvent {
  session_id: string;
  sender_device_id: string;
  sender_alias: string;
  network_route: 'lan_direct' | 'wan_p2p';
  total_files: number;
  total_size_bytes: number;
  files: ManifestFileEntry[];
}

export interface TransferRecord {
  id: string;
  peer_device_id: string;
  peer_alias: string;
  direction: 'incoming' | 'outgoing';
  network_route: 'lan_direct' | 'wan_p2p';
  total_files: number;
  total_bytes: number;
  transferred_bytes: number;
  status: 'in_progress' | 'paused' | 'completed' | 'cancelled' | 'failed' | 'rejected';
  target_path: string | null;
  created_at: number;
  speed_mbps?: number;
  eta_seconds?: number;
}

export interface TransferProgressEvent {
  session_id: string;
  peer_device_id: string;
  peer_alias: string;
  direction: 'incoming' | 'outgoing';
  network_route: 'lan_direct' | 'wan_p2p';
  transferred_bytes: number;
  total_bytes: number;
  speed_mbps: number;
  eta_seconds: number;
  status: string;
}

export function getApiBaseUrl(signalingUrl: string): string {
  const trimmed = (signalingUrl || 'wss://raydrop.gapleh.online/ws').trim();
  return trimmed
    .replace(/^wss:\/\//i, 'https://')
    .replace(/^ws:\/\//i, 'http://')
    .replace(/\/ws\/?$/i, '');
}

export const raydropApi = {
  getAppState: () => invoke<AppStateSnapshot>('get_app_state'),
  saveCloudSession: (params: {
    userId: string;
    userEmail: string;
    userName: string;
    authToken: string;
    deviceName?: string;
  }) => invoke<AppSettings>('save_cloud_session', params),
  logoutCloudSession: () => invoke<AppSettings>('logout_cloud_session'),
  syncCloudDataToLocal: (
    myDevices: CloudDeviceNode[],
    contacts: CloudContactGroup[]
  ) => invoke<Contact[]>('sync_cloud_data_to_local', { myDevices, contacts }),
  deleteLocalContact: (params: { deviceId?: string; email?: string }) =>
    invoke<void>('delete_local_contact', params),
  toggleDiscoverability: (enabled: boolean) =>
    invoke<boolean>('toggle_discoverability', { enabled }),
  toggleAutostart: (enabled: boolean) =>
    invoke<boolean>('toggle_autostart', { enabled }),
  updateSettings: (params: {
    aliasName: string;
    downloadDir: string;
    signalingUrl: string;
    stunAddr: string;
    autostart?: boolean;
  }) => invoke<AppSettings>('update_settings', params),
  getContacts: () => invoke<Contact[]>('get_contacts'),
  addWanContact: (params: {
    deviceId: string;
    aliasName: string;
    directIpPort?: string | null;
  }) => invoke<Contact>('add_wan_contact', params),
  updateContactTrust: (
    deviceId: string,
    trustLevel: Contact['trust_level'],
    userEmail?: string | null
  ) => invoke<void>('update_contact_trust', { deviceId, trustLevel, userEmail }),
  initiateTransfer: (targetDeviceId: string, filePaths: string[]) =>
    invoke<string>('initiate_transfer', { targetDeviceId, filePaths }),
  respondTransferRequest: (
    sessionId: string,
    accepted: boolean,
    targetDirectory?: string | null
  ) =>
    invoke<void>('respond_transfer_request', {
      sessionId,
      accepted,
      targetDirectory,
    }),
  cancelTransfer: (sessionId: string) =>
    invoke<void>('cancel_transfer', { sessionId }),
  getTransferHistory: () => invoke<TransferRecord[]>('get_transfer_history'),
  registerOsContextMenu: () => invoke<string>('register_os_context_menu'),
  listenEvent: <T>(event: string, handler: (payload: T) => void): Promise<UnlistenFn> =>
    listen<T>(event, (e) => handler(e.payload)),
};

export const cloudApi = {
  async register(
    baseUrl: string,
    payload: {
      email: string;
      name: string;
      password: string;
      device_id: string;
      device_name: string;
      os_type: string;
      ed25519_pubkey: string;
    }
  ): Promise<CloudStateResponse> {
    const res = await fetch(`${baseUrl}/api/auth/register`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
    });
    const data = await res.json();
    if (!res.ok) throw new Error(data.error || 'Registration failed');
    return data;
  },

  async login(
    baseUrl: string,
    payload: {
      email: string;
      password: string;
      device_id: string;
      device_name: string;
      os_type: string;
      ed25519_pubkey: string;
    }
  ): Promise<CloudStateResponse> {
    const res = await fetch(`${baseUrl}/api/auth/login`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
    });
    const data = await res.json();
    if (!res.ok) throw new Error(data.error || 'Login failed');
    return data;
  },

  async getState(baseUrl: string, token: string): Promise<CloudStateResponse> {
    const res = await fetch(`${baseUrl}/api/state`, {
      headers: { Authorization: `Bearer ${token}` },
    });
    const data = await res.json();
    if (!res.ok) throw new Error(data.error || 'Failed to fetch cloud state');
    return data;
  },

  async renameDevice(
    baseUrl: string,
    token: string,
    deviceId: string,
    deviceName: string
  ): Promise<{ my_devices: CloudDeviceNode[] }> {
    const res = await fetch(`${baseUrl}/api/devices/${encodeURIComponent(deviceId)}`, {
      method: 'PATCH',
      headers: {
        'Content-Type': 'application/json',
        Authorization: `Bearer ${token}`,
      },
      body: JSON.stringify({ device_name: deviceName }),
    });
    const data = await res.json();
    if (!res.ok) throw new Error(data.error || 'Failed to rename device');
    return data;
  },

  async deleteDevice(
    baseUrl: string,
    token: string,
    deviceId: string
  ): Promise<{ my_devices: CloudDeviceNode[] }> {
    const res = await fetch(`${baseUrl}/api/devices/${encodeURIComponent(deviceId)}`, {
      method: 'DELETE',
      headers: { Authorization: `Bearer ${token}` },
    });
    const data = await res.json();
    if (!res.ok) throw new Error(data.error || 'Failed to delete device');
    return data;
  },

  async addContactByEmail(
    baseUrl: string,
    token: string,
    email: string
  ): Promise<{ contacts: CloudContactGroup[] }> {
    const res = await fetch(`${baseUrl}/api/contacts`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        Authorization: `Bearer ${token}`,
      },
      body: JSON.stringify({ email }),
    });
    const data = await res.json();
    if (!res.ok) throw new Error(data.error || 'Failed to add contact');
    return data;
  },

  async updateContactTrust(
    baseUrl: string,
    token: string,
    email: string,
    trustLevel: string
  ): Promise<{ contacts: CloudContactGroup[] }> {
    const res = await fetch(`${baseUrl}/api/contacts`, {
      method: 'PATCH',
      headers: {
        'Content-Type': 'application/json',
        Authorization: `Bearer ${token}`,
      },
      body: JSON.stringify({ email, trust_level: trustLevel }),
    });
    const data = await res.json();
    if (!res.ok) throw new Error(data.error || 'Failed to update contact trust');
    return data;
  },

  async deleteContactByEmail(
    baseUrl: string,
    token: string,
    email: string
  ): Promise<{ contacts: CloudContactGroup[] }> {
    const res = await fetch(`${baseUrl}/api/contacts/${encodeURIComponent(email)}`, {
      method: 'DELETE',
      headers: { Authorization: `Bearer ${token}` },
    });
    const data = await res.json();
    if (!res.ok) throw new Error(data.error || 'Failed to remove contact');
    return data;
  },
};

export function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${(bytes / Math.pow(k, i)).toFixed(i === 0 ? 0 : 2)} ${sizes[i]}`;
}
