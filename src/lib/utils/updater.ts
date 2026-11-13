import {
  updateInfo,
  updateModalOpen,
  updateChecking,
  updateErrorMessage,
  updateStatusMessage,
} from '../stores/update';
import { raydropApi } from './tauri-ipc';
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from '@tauri-apps/plugin-notification';

export async function checkForUpdates(manual: boolean = false): Promise<boolean> {
  try {
    updateChecking.set(true);
    updateErrorMessage.set('');

    const res = await raydropApi.checkAppUpdate();
    updateInfo.set(res);

    if (res.has_update) {
      updateModalOpen.set(true);

      // Trigger native OS notification
      try {
        let hasPermission = await isPermissionGranted();
        if (!hasPermission) {
          const perm = await requestPermission();
          hasPermission = perm === 'granted';
        }
        if (hasPermission) {
          sendNotification({
            title: `Raydrop Update: ${res.latest_version}`,
            body: `A new version of Raydrop is ready. Open the app to update now!`,
          });
        }
      } catch (err) {
        console.warn('Could not dispatch OS update notification:', err);
      }
      return true;
    } else {
      if (manual) {
        updateStatusMessage.set(`Raydrop v${res.current_version} is up to date!`);
        setTimeout(() => updateStatusMessage.set(''), 4000);
      }
      return false;
    }
  } catch (err: any) {
    console.warn('Update check failed:', err);
    if (manual) {
      const msg = typeof err === 'string' ? err : err?.message || 'Failed to check for updates';
      updateErrorMessage.set(msg);
      setTimeout(() => updateErrorMessage.set(''), 4000);
    }
    return false;
  } finally {
    updateChecking.set(false);
  }
}

