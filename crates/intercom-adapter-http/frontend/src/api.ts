import { createDashboardApi } from '@embedded-device-foundation/dashboard-core';
import type { SystemDiagnostics } from './types';

const dashboardApi = createDashboardApi<SystemDiagnostics>();

async function preserveHttpError<T>(
  request: () => Promise<T>,
  format: (status: number) => string,
): Promise<T> {
  try {
    return await request();
  } catch (error: unknown) {
    const match = error instanceof Error ? /^HTTP (\d+):/.exec(error.message) : null;
    if (!match) throw error;
    throw new Error(format(Number(match[1])));
  }
}

export async function fetchStatus(): Promise<SystemDiagnostics> {
  return preserveHttpError(dashboardApi.fetchStatus, (status) => `HTTP error ${status}`);
}

export async function fetchLogs(): Promise<string> {
  return preserveHttpError(dashboardApi.fetchLogs, (status) => `HTTP error ${status}`);
}

export async function triggerUnlock(): Promise<void> {
  const res = await fetch('/api/action/unlock', { method: 'POST' });
  if (!res.ok) {
    throw new Error(`Unlock failed: HTTP ${res.status}`);
  }
}

export async function triggerRing(): Promise<void> {
  const res = await fetch('/api/action/ring', { method: 'POST' });
  if (!res.ok) {
    throw new Error(`Ring failed: HTTP ${res.status}`);
  }
}

export async function startBusListen(): Promise<void> {
  const res = await fetch('/api/action/listen', { method: 'POST' });
  if (!res.ok) {
    throw new Error(`Listen failed: HTTP ${res.status}`);
  }
}

export async function startBusTalk(): Promise<void> {
  const res = await fetch('/api/action/talk/start', { method: 'POST' });
  if (!res.ok) {
    throw new Error(`Start talk failed: HTTP ${res.status}`);
  }
}

export async function stopBusTalk(): Promise<void> {
  const res = await fetch('/api/action/talk/stop', { method: 'POST' });
  if (!res.ok) {
    throw new Error(`Stop talk failed: HTTP ${res.status}`);
  }
}

export async function sendBusHeartbeat(): Promise<void> {
  const res = await fetch('/api/action/heartbeat', { method: 'POST' });
  if (!res.ok) {
    throw new Error(`Heartbeat failed: HTTP ${res.status}`);
  }
}

export async function hangupBusSession(): Promise<void> {
  const res = await fetch('/api/action/hangup', { method: 'POST' });
  if (!res.ok) {
    throw new Error(`Hangup failed: HTTP ${res.status}`);
  }
}

export async function setDoorUnlockDuration(seconds: number): Promise<void> {
  const res = await fetch(`/api/action/door-duration?seconds=${seconds}`, { method: 'POST' });
  if (!res.ok) {
    throw new Error(`Failed to update duration: HTTP ${res.status}`);
  }
}

export async function resetMatterKvs(): Promise<void> {
  return preserveHttpError(
    dashboardApi.resetMatterKvs,
    (status) => `Matter KVS reset failed: HTTP ${status}`,
  );
}

export async function reboot(): Promise<void> {
  return preserveHttpError(dashboardApi.reboot, (status) => `Reboot failed: HTTP ${status}`);
}

export async function factoryReset(): Promise<void> {
  return preserveHttpError(
    dashboardApi.factoryReset,
    (status) => `Factory reset failed: HTTP ${status}`,
  );
}

export async function setAudioGain(rxHalfDb: number, txHalfDb: number): Promise<void> {
  const res = await fetch(
    `/api/action/audio/gain?rx_half_db=${encodeURIComponent(rxHalfDb)}&tx_half_db=${encodeURIComponent(txHalfDb)}`,
    { method: 'POST' },
  );
  if (!res.ok) {
    throw new Error(`Audio gain update failed: HTTP ${res.status}`);
  }
}

export async function setBenchTone(enable: boolean): Promise<void> {
  const res = await fetch(`/api/debug/audio/tone?enable=${enable ? 'true' : 'false'}`, {
    method: 'POST',
  });
  if (!res.ok) {
    throw new Error(`Bench tone update failed: HTTP ${res.status}`);
  }
}
