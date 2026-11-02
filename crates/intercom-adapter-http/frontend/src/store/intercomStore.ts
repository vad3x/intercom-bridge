import { createEffect, createSignal, onCleanup } from 'solid-js';
import {
  factoryReset,
  fetchStatus,
  hangupBusSession,
  reboot,
  resetMatterKvs,
  sendBusHeartbeat,
  setAudioGain,
  setBenchTone,
  setDoorUnlockDuration,
  startBusListen,
  startBusTalk,
  stopBusTalk,
  triggerRing,
  triggerUnlock,
} from '../api';
import type { SystemDiagnostics } from '../types';

const DEBUG_SESSION_STARTUP_GRACE_MS = 5000;

export function createIntercomStore() {
  const [status, setStatus] = createSignal<SystemDiagnostics | null>(null);
  const [doorUnlockDurationSec, setDoorUnlockDurationSec] = createSignal<number | undefined>(
    undefined,
  );
  const [isUnlocking, setIsUnlocking] = createSignal(false);
  const [isRinging, setIsRinging] = createSignal(false);
  const [isResettingMatter, setIsResettingMatter] = createSignal(false);
  const [debugSessionActive, setDebugSessionActive] = createSignal(false);
  const [desiredAudioGain, setDesiredAudioGain] = createSignal<AudioGainUpdate | null>(null);
  let lastPostedAudioGain: AudioGainUpdate | null = null;
  let audioGainGeneration = 0;
  let completedAudioGainGeneration = 0;
  let audioGainQueue = Promise.resolve();
  let latestStatusPoll = 0;
  let debugSessionGeneration = 0;
  let debugSessionStartupDeadline = 0;
  let debugSessionAcknowledged = false;
  let pollSequence = 0;
  let latestSessionPoll = 0;

  const poll = async () => {
    if (typeof document !== 'undefined' && document.hidden) {
      return;
    }
    const generation = debugSessionGeneration;
    const gainGeneration = audioGainGeneration;
    const completedGainGeneration = completedAudioGainGeneration;
    const sequence = ++pollSequence;
    try {
      const data = await fetchStatus();
      if (sequence < latestStatusPoll) return;
      latestStatusPoll = sequence;
      setStatus(data);
      const desired = desiredAudioGain();
      // Gain routes queue commands. Keep the desired or rollback pair until a
      // poll started after its successful POST observes it.
      if (
        desired &&
        gainGeneration === audioGainGeneration &&
        completedGainGeneration === desired.generation &&
        data.rx_gain_half_db === desired.rxHalfDb &&
        data.tx_gain_half_db === desired.txHalfDb
      ) {
        setDesiredAudioGain(null);
        if (lastPostedAudioGain?.generation === desired.generation) lastPostedAudioGain = null;
      }
      if (data.door_unlock_duration_sec !== undefined) {
        setDoorUnlockDurationSec(data.door_unlock_duration_sec);
      }
      if (data.door_locked && isUnlocking()) {
        setIsUnlocking(false);
      }
      if (
        generation === debugSessionGeneration &&
        sequence > latestSessionPoll &&
        debugSessionActive()
      ) {
        latestSessionPoll = sequence;
        if (data.call_state !== undefined && data.call_state !== 'idle') {
          debugSessionAcknowledged = true;
        }
        // Start routes queue commands; idle may persist until the controller observes them.
        if (
          data.call_state === 'idle' &&
          (debugSessionAcknowledged || Date.now() >= debugSessionStartupDeadline)
        ) {
          setDebugSessionActive(false);
        }
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
      const generation = debugSessionGeneration;
      void sendBusHeartbeat().catch(() => {
        if (generation === debugSessionGeneration) {
          setDebugSessionActive(false);
        }
      });
    }, 2000);
    onCleanup(() => clearInterval(timer));
  });

  const startListen = async () => {
    await startBusListen();
    debugSessionGeneration += 1;
    debugSessionStartupDeadline = Date.now() + DEBUG_SESSION_STARTUP_GRACE_MS;
    debugSessionAcknowledged = false;
    setDebugSessionActive(true);
    await poll();
  };

  const startTalk = async () => {
    await startBusTalk();
    debugSessionGeneration += 1;
    debugSessionStartupDeadline = Date.now() + DEBUG_SESSION_STARTUP_GRACE_MS;
    debugSessionAcknowledged = false;
    setDebugSessionActive(true);
    await poll();
  };

  const stopTalk = async () => {
    await stopBusTalk();
    await poll();
  };

  const hangupSession = async () => {
    debugSessionGeneration += 1;
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

  const rxGainHalfDb = () => desiredAudioGain()?.rxHalfDb ?? status()?.rx_gain_half_db;
  const txGainHalfDb = () => desiredAudioGain()?.txHalfDb ?? status()?.tx_gain_half_db;

  const updateAudioGain = async (rxHalfDb: number, txHalfDb: number) => {
    const generation = ++audioGainGeneration;
    // Publish synchronously so the other slider combines with this edit.
    const update = { rxHalfDb, txHalfDb, generation };
    setDesiredAudioGain(update);
    const write = audioGainQueue.then(async () => {
      await setAudioGain(rxHalfDb, txHalfDb);
      completedAudioGainGeneration = generation;
      lastPostedAudioGain = update;
    });
    // A failed request must not prevent a later queued edit from being sent.
    audioGainQueue = write.catch(() => {});
    try {
      await write;
      await poll();
    } catch (error: unknown) {
      // A previous POST may be accepted while status still predates it. Retain
      // that successful pair until observed instead of reverting to stale status.
      if (generation === audioGainGeneration) setDesiredAudioGain(lastPostedAudioGain);
      throw error;
    }
  };

  const toggleBenchTone = async (enable: boolean) => {
    await setBenchTone(enable);
    await poll();
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
    rxGainHalfDb,
    txGainHalfDb,
    updateAudioGain,
    toggleBenchTone,
    startListen,
    startTalk,
    stopTalk,
    hangupSession,
  };
}

interface AudioGainUpdate {
  rxHalfDb: number;
  txHalfDb: number;
  generation: number;
}
