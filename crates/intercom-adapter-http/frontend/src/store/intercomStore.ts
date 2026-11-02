import { createEffect, createSignal, onCleanup } from 'solid-js';
import {
  factoryReset,
  fetchStatus,
  hangupBusSession,
  reboot,
  resetMatterKvs,
  sendBusHeartbeat,
  setDoorUnlockDuration,
  startBusListen,
  startBusTalk,
  stopBusTalk,
  triggerRing,
  triggerUnlock,
} from '../api';
import type { SystemDiagnostics } from '../types';

export function createIntercomStore() {
  const [status, setStatus] = createSignal<SystemDiagnostics | null>(null);
  const [doorUnlockDurationSec, setDoorUnlockDurationSec] = createSignal<number | undefined>(
    undefined,
  );
  const [isUnlocking, setIsUnlocking] = createSignal(false);
  const [isRinging, setIsRinging] = createSignal(false);
  const [isResettingMatter, setIsResettingMatter] = createSignal(false);
  const [debugSessionActive, setDebugSessionActive] = createSignal(false);

  const poll = async () => {
    if (typeof document !== 'undefined' && document.hidden) {
      return;
    }
    try {
      const data = await fetchStatus();
      setStatus(data);
      if (data.door_unlock_duration_sec !== undefined) {
        setDoorUnlockDurationSec(data.door_unlock_duration_sec);
      }
      if (data.door_locked && isUnlocking()) {
        setIsUnlocking(false);
      }
      if (data.call_state === 'idle' && debugSessionActive()) {
        setDebugSessionActive(false);
      }
    } catch {
      // Offline / server unreachable
    }
  };

  createEffect(() => {
    if (!debugSessionActive()) {
      return;
    }
    const timer = setInterval(() => {
      void sendBusHeartbeat().catch(() => {
        setDebugSessionActive(false);
      });
    }, 2000);
    onCleanup(() => clearInterval(timer));
  });

  const startListen = async () => {
    await startBusListen();
    setDebugSessionActive(true);
    await poll();
  };

  const startTalk = async () => {
    await startBusTalk();
    setDebugSessionActive(true);
    await poll();
  };

  const stopTalk = async () => {
    await stopBusTalk();
    await poll();
  };

  const hangupSession = async () => {
    setDebugSessionActive(false);
    await hangupBusSession();
    await poll();
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

  const updateDoorUnlockDuration = async (seconds: number) => {
    const previous = doorUnlockDurationSec();
    setDoorUnlockDurationSec(seconds);
    try {
      await setDoorUnlockDuration(seconds);
      await poll();
    } catch (e) {
      setDoorUnlockDurationSec(previous);
      throw e;
    }
  };

  const rebootDevice = async () => {
    await reboot();
  };

  const factoryResetDevice = async () => {
    await factoryReset();
  };

  return {
    status,
    doorUnlockDurationSec,
    isUnlocking,
    isRinging,
    isResettingMatter,
    debugSessionActive,
    poll,
    unlockDoor,
    testRing,
    resetMatter,
    rebootDevice,
    factoryResetDevice,
    updateDoorUnlockDuration,
    startListen,
    startTalk,
    stopTalk,
    hangupSession,
  };
}
