import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export interface AppSettings {
  device_id: string;
  alias_name: string;
  discoverable: boolean;
  download_dir: string;
  signaling_url: string;
  stun_addr: string;
}

export interface AppStateSnapshot {
  settings: AppSettings;
  local_ip: string;
  listen_port: number;
  wan_reflexive_addr: string | null;
  signaling_connected: boolean;
  queued_cli_files: string[];
}

export interface Contact {
  device_id: string;
  alias_name: string;
  ed25519_pubkey: string | null;
  network_type: 'lan' | 'wan';
  last_known_ip: string | null;
  last_known_port: number | null;
  trust_level: 'ask' | 'trusted_auto_accept' | 'blocked';
  is_online: boolean;
  last_seen_at: number;
  created_at: number;
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

export const raydropApi = {
  getAppState: () => invoke<AppStateSnapshot>('get_app_state'),
  toggleDiscoverability: (enabled: boolean) =>
    invoke<boolean>('toggle_discoverability', { enabled }),
  updateSettings: (params: {
    aliasName: string;
    downloadDir: string;
    signalingUrl: string;
    stunAddr: string;
  }) => invoke<AppSettings>('update_settings', params),
  getContacts: () => invoke<Contact[]>('get_contacts'),
  addWanContact: (params: {
    deviceId: string;
    aliasName: string;
    directIpPort?: string | null;
  }) => invoke<Contact>('add_wan_contact', params),
  updateContactTrust: (deviceId: string, trustLevel: Contact['trust_level']) =>
    invoke<void>('update_contact_trust', { deviceId, trustLevel }),
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

export function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${(bytes / Math.pow(k, i)).toFixed(i === 0 ? 0 : 2)} ${sizes[i]}`;
}

