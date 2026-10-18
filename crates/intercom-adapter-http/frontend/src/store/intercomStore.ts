import { createSignal } from 'solid-js';
import { createStore, reconcile } from 'solid-js/store';
import { fetchStatus, resetMatterKvs, triggerRing, triggerUnlock } from '../api';
import type { SystemDiagnostics } from '../types';

export function createIntercomStore() {
  const [store, setStore] = createStore<{ status: SystemDiagnostics | null }>({
    status: null,
  });
  const [isUnlocking, setIsUnlocking] = createSignal(false);
  const [isRinging, setIsRinging] = createSignal(false);
  const [isResettingMatter, setIsResettingMatter] = createSignal(false);

  const poll = async () => {
    if (typeof document !== 'undefined' && document.hidden) return;
    try {
      const data = await fetchStatus();
      if (!store.status) {
        setStore('status', data);
      } else {
        setStore('status', reconcile(data));
      }
      // If hardware reports locked and unlocking intent expired, clear unlocking
      if (data.door_locked && isUnlocking()) {
        setIsUnlocking(false);
      }
    } catch {
      // Offline / server unreachable
    }
  };

  const unlockDoor = async () => {
    setIsUnlocking(true);
    try {
      await triggerUnlock();
      await poll();
    } catch (e) {
      setIsUnlocking(false);
      throw e;
    }
  };

  const testRing = async () => {
    setIsRinging(true);
    try {
      await triggerRing();
      setTimeout(() => setIsRinging(false), 3000);
    } catch (e) {
      setIsRinging(false);
      throw e;
    }
  };

  const resetMatter = async () => {
    setIsResettingMatter(true);
    try {
      await resetMatterKvs();
    } catch (e) {
      setIsResettingMatter(false);
      throw e;
    }
  };

  return {
    status: () => store.status,
    isUnlocking,
    isRinging,
    isResettingMatter,
    poll,
    unlockDoor,
    testRing,
    resetMatter,
  };
}
