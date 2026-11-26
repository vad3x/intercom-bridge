import { afterEach, describe, expect, mock, test } from 'bun:test';

function makeStatusPayload(doorLocked: boolean, doorUnlockDurationSec: number) {
  return {
    system: {
      status: 'Operational',
      uptime_sec: 14250,
      memory: {
        internal: { free_bytes: 151552, total_bytes: 215040 },
        psram: null,
      },
      cpu: {},
    },
    network: {
      ssid: 'MyHomeNetwork',
      rssi_dbm: -58,
      ip: '192.168.1.142',
      mac: '68:B6:B3:21:40:8C',
    },
    firmware: {
      version: '1.0.0',
      git_commit: 'a3f89b',
      build_date: '2026-11-26',
      active_slot: 'ota_0',
      slot_valid: true,
    },
    ecosystems: {
      available: ['Matter'],
      active: null,
    },
    device: {
      type: 'intercom_bridge',
      state: {
        door_locked: doorLocked,
        door_unlock_duration_sec: doorUnlockDurationSec,
      },
    },
  };
}

mock.module('@embedded-device-foundation/dashboard-core', () => ({
  createDashboardApi: <T>() => ({
    fetchStatus: async (): Promise<T> => {
      const res = await fetch('/api/status');
      if (!res.ok) {
        throw new Error(`HTTP ${res.status}: ${res.statusText}`);
      }
      return (await res.json()) as T;
    },
    fetchLogs: async (): Promise<string> => '',
    resetMatterKvs: async (): Promise<void> => {},
    reboot: async (): Promise<void> => {},
    factoryReset: async (): Promise<void> => {},
  }),
}));

const { fetchStatus } = await import('../src/api');
const { createIntercomStore } = await import('../src/store/intercomStore');

const originalFetch = globalThis.fetch;

afterEach(() => {
  globalThis.fetch = originalFetch;
});

describe('v1 status API & store contract', () => {
  test('fetchStatus parses five-bucket schema and device.state', async () => {
    globalThis.fetch = (async () =>
      Response.json(makeStatusPayload(true, 5))) as unknown as typeof fetch;

    const status = await fetchStatus();

    expect(status.system.status).toBe('Operational');
    expect(status.system.memory.internal).toEqual({
      free_bytes: 151552,
      total_bytes: 215040,
    });
    expect(status.system.memory.psram).toBeNull();
    expect(status.system.cpu).toEqual({});
    expect(status.network.mac).toBe('68:B6:B3:21:40:8C');
    expect(status.firmware.active_slot).toBe('ota_0');
    expect(status.ecosystems.available).toEqual(['Matter']);
    expect(status.device.type).toBe('intercom_bridge');
    expect(status.device.state).toEqual({
      door_locked: true,
      door_unlock_duration_sec: 5,
    });
  });

  test('createIntercomStore().poll() reads door_unlock_duration_sec and door_locked from device.state', async () => {
    let locked = false;
    globalThis.fetch = (async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url === '/api/action/unlock') {
        return Response.json({ status: 'ok' });
      }
      return Response.json(makeStatusPayload(locked, 7));
    }) as unknown as typeof fetch;

    const store = createIntercomStore();
    await store.unlockDoor();

    expect(store.doorUnlockDurationSec()).toBe(7);
    expect(store.isUnlocking()).toBe(true);

    locked = true;
    await store.poll();

    expect(store.isUnlocking()).toBe(false);
    expect(store.status()?.device.state.door_locked).toBe(true);
  });

  test('TypeScript contract rejects legacy flat keys and accepts five-bucket IntercomDiagnostics', () => {
    const tsc = Bun.spawnSync(
      [
        `${import.meta.dir}/../node_modules/.bin/tsc`,
        '--ignoreConfig',
        '--noEmit',
        '--strict',
        '--target',
        'ESNext',
        '--module',
        'ESNext',
        '--moduleResolution',
        'bundler',
        '--jsx',
        'preserve',
        '--jsxImportSource',
        'solid-js',
        '--skipLibCheck',
        `${import.meta.dir}/types-contract.ts`,
      ],
      { cwd: `${import.meta.dir}/..` },
    );
    const output = `${tsc.stdout.toString()}${tsc.stderr.toString()}`;
    expect(tsc.exitCode, output).toBe(0);
  });
});
