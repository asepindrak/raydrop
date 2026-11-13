import { writable, derived } from 'svelte/store';
import { raydropApi, type ChatMessage } from '../utils/tauri-ipc';

export const chatStore = writable<Record<string, ChatMessage[]>>({});
export const unreadCounts = writable<Record<string, number>>({});

export const totalUnreadCount = derived(unreadCounts, ($counts) => {
  return Object.values($counts).reduce((acc, curr) => acc + (curr || 0), 0);
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
    const msgs = await raydropApi.getChatMessages(peerDeviceId, 300, 0);
    chatStore.update((map) => ({ ...map, [peerDeviceId]: msgs }));
    await markChatRead(peerDeviceId);
  } catch (err) {
    console.warn('Failed to load chat history:', err);
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

