import { writable } from 'svelte/store';
import type { UpdateCheckResult, UpdateProgressPayload } from '../utils/tauri-ipc';

export const updateInfo = writable<UpdateCheckResult | null>(null);
export const updateModalOpen = writable<boolean>(false);
export const updateChecking = writable<boolean>(false);
export const updateDownloading = writable<boolean>(false);
export const updateProgress = writable<UpdateProgressPayload | null>(null);
export const updateStatusMessage = writable<string>('');
export const updateErrorMessage = writable<string>('');
