import { createSignal } from 'solid-js';
import { fetchStatus, triggerRing, triggerUnlock } from '../api';
import type { SystemDiagnostics } from '../types';

export function createIntercomStore() {
  const [status, setStatus] = createSignal<SystemDiagnostics | null>(null);
  const [isUnlocking, setIsUnlocking] = createSignal(false);
  const [isRinging, setIsRinging] = createSignal(false);

  const poll = async () => {
    if (typeof document !== 'undefined' && document.hidden) return;
    try {
      const data = await fetchStatus();
      setStatus(data);
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

  return {
    status,
    isUnlocking,
    isRinging,
    poll,
    unlockDoor,
    testRing,
  };
}
