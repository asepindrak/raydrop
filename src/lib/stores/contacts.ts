import { writable, derived } from 'svelte/store';
import type { Contact } from '../utils/tauri-ipc';

export const contacts = writable<Contact[]>([]);

export const lanContacts = derived(contacts, ($contacts) =>
  $contacts.filter((c) => c.network_type === 'lan')
);

export const wanContacts = derived(contacts, ($contacts) =>
  $contacts.filter((c) => c.network_type === 'wan')
);

