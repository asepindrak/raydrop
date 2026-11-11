import { writable } from 'svelte/store';
import type { AppSettings, IncomingTransferEvent } from '../utils/tauri-ipc';

export const appSettings = writable<AppSettings>({
  device_id: 'RAY-0000-0000-0000',
  alias_name: 'Raydrop-Device',
  discoverable: true,
  download_dir: '',
  signaling_url: 'wss://raydrop.gapleh.online/ws',
  stun_addr: 'raydrop.gapleh.online:3478',
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

export const incomingRequestQueue = writable<IncomingTransferEvent[]>([]);
export const cliQueuedFiles = writable<string[]>([]);
export const settingsModalOpen = writable<boolean>(false);

