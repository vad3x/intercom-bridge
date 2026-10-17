import type { SystemDiagnostics } from './types';

export async function fetchStatus(): Promise<SystemDiagnostics> {
  const res = await fetch('/api/status');
  if (!res.ok) {
    throw new Error(`HTTP error ${res.status}`);
  }
  return res.json();
}

export async function fetchLogs(): Promise<string> {
  const res = await fetch('/api/logs');
  if (!res.ok) {
    throw new Error(`HTTP error ${res.status}`);
  }
  return res.text();
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

export async function resetMatterKvs(): Promise<void> {
  const res = await fetch('/api/action/reset/matter', { method: 'POST' });
  if (!res.ok) {
    throw new Error(`Matter KVS reset failed: HTTP ${res.status}`);
  }
}
