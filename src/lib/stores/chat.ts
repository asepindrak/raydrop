import { writable, derived } from 'svelte/store';
import { raydropApi, type ChatMessage } from '../utils/tauri-ipc';

export const chatStore = writable<Record<string, ChatMessage[]>>({});
export const unreadCounts = writable<Record<string, number>>({});
export const hasMoreMap = writable<Record<string, boolean>>({});
export const isLoadingOlder = writable<boolean>(false);

export interface ChatToast {
  id: string;
  peerDeviceId: string;
  peerName: string;
  content: string;
  timestamp: number;
}

export const chatToasts = writable<ChatToast[]>([]);

export const totalUnreadCount = derived(unreadCounts, ($counts) => {
  return Object.values($counts).reduce((acc, curr) => acc + (curr || 0), 0);
});

// Sync total unread count to System Tray icon badge & taskbar badge
totalUnreadCount.subscribe((count) => {
  raydropApi.updateTrayBadge(count).catch(() => {});
});

export async function fetchUnreadCounts() {
  try {
    const counts = await raydropApi.getUnreadChatCounts();
    unreadCounts.set(counts);
  } catch (err) {
    console.warn('Failed to get unread counts:', err);
  }
}

export async function loadChatHistory(peerDeviceId: string) {
  try {
    const limit = 40;
    const msgs = await raydropApi.getChatMessages(peerDeviceId, limit, 0);
    chatStore.update((map) => ({ ...map, [peerDeviceId]: msgs }));
    hasMoreMap.update((m) => ({ ...m, [peerDeviceId]: msgs.length >= limit }));
    await markChatRead(peerDeviceId);
  } catch (err) {
    console.warn('Failed to load chat history:', err);
  }
}

export async function loadOlderMessages(peerDeviceId: string): Promise<number> {
  let count = 0;
  chatStore.update((map) => {
    count = (map[peerDeviceId] || []).length;
    return map;
  });
  if (count === 0) return 0;

  isLoadingOlder.set(true);
  try {
    const limit = 40;
    const olderMsgs = await raydropApi.getChatMessages(peerDeviceId, limit, count);
    if (olderMsgs.length < limit) {
      hasMoreMap.update((m) => ({ ...m, [peerDeviceId]: false }));
    }
    if (olderMsgs.length > 0) {
      chatStore.update((map) => {
        const current = map[peerDeviceId] || [];
        const existingIds = new Set(current.map((m) => m.id));
        const filteredNew = olderMsgs.filter((m) => !existingIds.has(m.id));
        return {
          ...map,
          [peerDeviceId]: [...filteredNew, ...current],
        };
      });
      return olderMsgs.length;
    }
    return 0;
  } catch (err) {
    console.warn('Failed to load older messages:', err);
    return 0;
  } finally {
    isLoadingOlder.set(false);
  }
}

export async function markChatRead(peerDeviceId: string) {
  try {
    await raydropApi.markChatMessagesRead(peerDeviceId);
    unreadCounts.update((counts) => {
      const next = { ...counts };
      delete next[peerDeviceId];
      return next;
    });
    chatStore.update((map) => {
      const list = map[peerDeviceId];
      if (!list) return map;
      return {
        ...map,
        [peerDeviceId]: list.map((m) => ({ ...m, is_read: true })),
      };
    });
  } catch (err) {
    console.warn('Failed to mark chat read:', err);
  }
}

export function handleIncomingChatMessage(msg: ChatMessage) {
  chatStore.update((map) => {
    const current = map[msg.peer_device_id] || [];
    if (current.some((m) => m.id === msg.id)) return map;
    return {
      ...map,
      [msg.peer_device_id]: [...current, msg],
    };
  });
  unreadCounts.update((counts) => ({
    ...counts,
    [msg.peer_device_id]: (counts[msg.peer_device_id] || 0) + 1,
  }));
}

export function handleOutgoingChatMessage(msg: ChatMessage) {
  chatStore.update((map) => {
    const current = map[msg.peer_device_id] || [];
    if (current.some((m) => m.id === msg.id)) return map;
    return {
      ...map,
      [msg.peer_device_id]: [...current, msg],
    };
  });
}

export async function clearChatHistory(peerDeviceId: string) {
  await raydropApi.clearChatMessages(peerDeviceId);
  chatStore.update((map) => ({ ...map, [peerDeviceId]: [] }));
}

export function addChatToast(msg: ChatMessage, peerName: string) {
  const toast: ChatToast = {
    id: msg.id,
    peerDeviceId: msg.peer_device_id,
    peerName,
    content: msg.content || (msg.msg_type === 'file_transfer' ? '📎 Mengirim file' : 'Pesan baru'),
    timestamp: Date.now(),
  };

  chatToasts.update((list) => [toast, ...list.slice(0, 2)]);

  setTimeout(() => {
    removeChatToast(toast.id);
  }, 6000);
}

export function removeChatToast(id: string) {
  chatToasts.update((list) => list.filter((t) => t.id !== id));
}

export function formatRelativeTime(unixSecs: number): string {
  const now = Math.floor(Date.now() / 1000);
  const diff = Math.max(0, now - unixSecs);

  if (diff < 5) return 'Baru saja';
  if (diff < 60) return `${diff} detik yang lalu`;
  const mins = Math.floor(diff / 60);
  if (mins < 60) return `${mins} menit yang lalu`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours} jam yang lalu`;

  const d = new Date(unixSecs * 1000);
  const day = d.getDate().toString().padStart(2, '0');
  const months = ['Jan', 'Feb', 'Mar', 'Apr', 'Mei', 'Jun', 'Jul', 'Agu', 'Sep', 'Okt', 'Nov', 'Des'];
  const month = months[d.getMonth()];
  const year = d.getFullYear();
  const hoursStr = d.getHours().toString().padStart(2, '0');
  const minsStr = d.getMinutes().toString().padStart(2, '0');
  return `${day} ${month} ${year}, ${hoursStr}:${minsStr}`;
}
