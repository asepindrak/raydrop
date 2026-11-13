import { writable, derived } from 'svelte/store';
import type { TransferRecord, TransferProgressEvent } from '../utils/tauri-ipc';

export const transfers = writable<TransferRecord[]>([]);

export function applyTransferProgress(evt: TransferProgressEvent) {
  transfers.update((list) => {
    const idx = list.findIndex((t) => t.id === evt.session_id);
    if (idx !== -1) {
      const existing = list[idx];
      const updated: TransferRecord = {
        ...existing,
        transferred_bytes: evt.transferred_bytes,
        total_bytes: evt.total_bytes > 0 ? evt.total_bytes : existing.total_bytes,
        speed_mbps: evt.speed_mbps,
        eta_seconds: evt.eta_seconds,
        status: (evt.status as TransferRecord['status']) || existing.status,
      };
      const copy = [...list];
      copy[idx] = updated;
      return copy;
    }
    return [
      {
        id: evt.session_id,
        peer_device_id: evt.peer_device_id,
        peer_alias: evt.peer_alias,
        direction: evt.direction,
        network_route: evt.network_route,
        total_files: 1,
        total_bytes: evt.total_bytes,
        transferred_bytes: evt.transferred_bytes,
        status: (evt.status as TransferRecord['status']) || 'in_progress',
        target_path: null,
        created_at: Math.floor(Date.now() / 1000),
        speed_mbps: evt.speed_mbps,
        eta_seconds: evt.eta_seconds,
      },
      ...list,
    ];
  });
}

export function upsertTransferRecord(rec: TransferRecord) {
  transfers.update((list) => {
    const idx = list.findIndex((t) => t.id === rec.id);
    if (idx !== -1) {
      const copy = [...list];
      copy[idx] = { ...copy[idx], ...rec };
      return copy;
    }
    return [rec, ...list];
  });
}

export const activeTransfers = derived(transfers, ($transfers) =>
  $transfers.filter((t) => t.status === 'in_progress' || t.status === 'paused')
);

export const historicalTransfers = derived(transfers, ($transfers) =>
  $transfers.filter((t) => t.status !== 'in_progress' && t.status !== 'paused')
);

