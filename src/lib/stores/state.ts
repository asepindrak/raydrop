import { writable } from 'svelte/store';
import type {
  AppSettings,
  CloudContactGroup,
  CloudDeviceNode,
  IncomingTransferEvent,
} from '../utils/tauri-ipc';

export const appSettings = writable<AppSettings>({
  device_id: 'RAY-0000-0000-0000',
  alias_name: 'Raydrop-Device',
  user_id: null,
  user_email: null,
  user_name: null,
  auth_token: null,
  discoverable: true,
  download_dir: '',
  signaling_url: 'wss://raydrop.gapleh.online/ws',
  stun_addr: 'raydrop.gapleh.online:3478',
});

export const deviceCryptoMeta = writable<{
  ed25519Pubkey: string;
  osType: string;
}>({
  ed25519Pubkey: '',
  osType: 'windows',
});

export const networkInfo = writable<{
  localIp: string;
  listenPort: number;
  wanReflexiveAddr: string | null;
  signalingConnected: boolean;
}>({
  localIp: '127.0.0.1',
  listenPort: 53317,
  wanReflexiveAddr: null,
  signalingConnected: false,
});

export const cloudMyDevices = writable<CloudDeviceNode[]>([]);
export const cloudContacts = writable<CloudContactGroup[]>([]);
export const incomingRequestQueue = writable<IncomingTransferEvent[]>([]);
export const cliQueuedFiles = writable<string[]>([]);
export const settingsModalOpen = writable<boolean>(false);
export const authModalOpen = writable<boolean>(false);
