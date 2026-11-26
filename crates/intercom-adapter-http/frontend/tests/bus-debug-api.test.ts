import { afterEach, expect, mock, spyOn, test } from 'bun:test';
import type { IntercomStatus } from '../src/types';

// The shared barrel also loads browser-only UI; these helpers only need fetch.
mock.module('@embedded-device-foundation/dashboard-core', () => ({
  createDashboardApi: () => ({
    fetchStatus: async () => (await fetch('/api/status')).json(),
  }),
}));
const api = await import('../src/api');
const { createIntercomStore } = await import('../src/store/intercomStore');

function makeStatus(deviceState: Partial<IntercomStatus> = {}) {
  return {
    system: {
      status: 'Operational',
      uptime_sec: 120,
      memory: {
        internal: { free_bytes: 12480, total_bytes: 122880 },
        psram: null,
      },
      cpu: {
        core_0: { load_pct: 25 },
      },
    },
    network: {
      ssid: 'Home_IoT_5G',
      rssi_dbm: -58,
      ip: '192.168.1.142',
      mac: '98:A3:16:A6:78:44',
    },
    firmware: {
      version: '2.1.0',
      git_commit: 'a1b2c3d',
      build_date: '2026-11-26',
      active_slot: 'ota_0',
      slot_valid: true,
    },
    ecosystems: {
      available: ['matter'],
      active: {
        type: 'matter',
        paired: true,
        properties: { door_lock_subscribed: true },
        fabrics: [],
        endpoints: [],
      },
    },
    device: {
      type: 'intercom_bridge',
      state: {
        door_locked: true,
        door_unlock_duration_sec: 5,
        bus_state: 'idle',
        call_state: 'idle',
        rx_rms_dbfs: -60,
        rx_peak_dbfs: -48,
        rx_gain_half_db: 40,
        tx_gain_half_db: 0,
        tone_active: false,
        ...deviceState,
      },
    },
  };
}

const actions = [
  ['startBusListen', '/api/action/listen', 'Listen failed'],
  ['startBusTalk', '/api/action/talk/start', 'Start talk failed'],
  ['stopBusTalk', '/api/action/talk/stop', 'Stop talk failed'],
  ['sendBusHeartbeat', '/api/action/heartbeat', 'Heartbeat failed'],
  ['hangupBusSession', '/api/action/hangup', 'Hangup failed'],
] as const;

afterEach(() => {
  fetchSpy?.mockRestore();
  clockSpy?.mockRestore();
});

let fetchSpy: ReturnType<typeof spyOn<typeof globalThis, 'fetch'>> | undefined;
let clockSpy: ReturnType<typeof spyOn<typeof Date, 'now'>> | undefined;

test.each(actions)('%s posts to %s', async (name, path) => {
  // Arrange
  const action = Reflect.get(api, name) as (() => Promise<void>) | undefined;
  expect(action).toBeFunction();
  fetchSpy = spyOn(globalThis, 'fetch').mockResolvedValue(new Response(null, { status: 204 }));

  // Act
  await action?.();

  // Assert
  expect(fetchSpy).toHaveBeenCalledTimes(1);
  expect(fetchSpy).toHaveBeenCalledWith(path, { method: 'POST' });
});

test.each(actions)('%s rejects non-OK responses', async (name, _path, message) => {
  // Arrange
  const action = Reflect.get(api, name) as (() => Promise<void>) | undefined;
  expect(action).toBeFunction();
  fetchSpy = spyOn(globalThis, 'fetch').mockResolvedValue(new Response(null, { status: 503 }));

  // Act
  const request = action?.();

  // Assert
  await expect(request).rejects.toThrow(`${message}: HTTP 503`);
});

test.each(['startListen', 'startTalk'] as const)(
  '%s keeps its heartbeat active when the queued command initially reports idle',
  async (action) => {
    // Arrange
    fetchSpy = spyOn(globalThis, 'fetch').mockImplementation(async (path) =>
      path === '/api/status'
        ? Response.json(makeStatus({ call_state: 'idle' }))
        : new Response(null, { status: 204 }),
    );
    const store = createIntercomStore();

    // Act
    await store[action]();

    // Assert
    expect(store.debugSessionActive()).toBe(true);
  },
);

test('idle from a deferred pre-session poll cannot end the new session', async () => {
  // Arrange
  const stale = Promise.withResolvers<Response>();
  fetchSpy = spyOn(globalThis, 'fetch')
    .mockReturnValueOnce(stale.promise)
    .mockResolvedValueOnce(new Response(null, { status: 204 }))
    .mockResolvedValueOnce(Response.json(makeStatus({ call_state: 'listening' })));
  const store = createIntercomStore();
  const oldPoll = store.poll();
  await store.startListen();

  // Act
  stale.resolve(Response.json(makeStatus({ call_state: 'idle' })));
  await oldPoll;

  // Assert
  expect(store.debugSessionActive()).toBe(true);
});

test('idle from a superseded session cannot end a newer session', async () => {
  // Arrange
  const stale = Promise.withResolvers<Response>();
  fetchSpy = spyOn(globalThis, 'fetch').mockImplementation(async (path) =>
    path === '/api/status'
      ? Response.json(makeStatus({ call_state: 'listening' }))
      : new Response(null, { status: 204 }),
  );
  const store = createIntercomStore();
  await store.startListen();
  fetchSpy.mockReturnValueOnce(stale.promise);
  const oldPoll = store.poll();
  await store.startTalk();

  // Act
  stale.resolve(Response.json(makeStatus({ call_state: 'idle' })));
  await oldPoll;

  // Assert
  expect(store.debugSessionActive()).toBe(true);
});

test('a later current-session idle observation ends the heartbeat session', async () => {
  // Arrange
  fetchSpy = spyOn(globalThis, 'fetch')
    .mockResolvedValueOnce(new Response(null, { status: 204 }))
    .mockResolvedValueOnce(Response.json(makeStatus({ call_state: 'listening' })))
    .mockResolvedValueOnce(Response.json(makeStatus({ call_state: 'idle' })));
  const store = createIntercomStore();
  await store.startListen();

  // Act
  await store.poll();

  // Assert
  expect(store.debugSessionActive()).toBe(false);
});

test('hangup ends the heartbeat session even when its request fails', async () => {
  // Arrange
  fetchSpy = spyOn(globalThis, 'fetch')
    .mockResolvedValueOnce(new Response(null, { status: 204 }))
    .mockResolvedValueOnce(Response.json(makeStatus({ call_state: 'listening' })))
    .mockResolvedValueOnce(new Response(null, { status: 503 }));
  const store = createIntercomStore();
  await store.startListen();

  // Act
  const request = store.hangupSession();

  // Assert
  await expect(request).rejects.toThrow('Hangup failed: HTTP 503');
  expect(store.debugSessionActive()).toBe(false);
});

test.each(['startListen', 'startTalk'] as const)(
  '%s survives a regular idle poll before its queued start is observed',
  async (action) => {
    // Arrange
    clockSpy = spyOn(Date, 'now').mockReturnValue(1000);
    fetchSpy = spyOn(globalThis, 'fetch').mockImplementation(async (path) =>
      path === '/api/status'
        ? Response.json(makeStatus({ call_state: 'idle' }))
        : new Response(null, { status: 204 }),
    );
    const store = createIntercomStore();
    await store[action]();

    // Act
    clockSpy.mockReturnValue(3500);
    await store.poll();

    // Assert
    expect(store.debugSessionActive()).toBe(true);
  },
);

test.each([
  [4999, true],
  [5000, false],
  [5001, false],
] as const)('unacknowledged startup at %d ms respects the deadline', async (elapsed, active) => {
  // Arrange
  clockSpy = spyOn(Date, 'now').mockReturnValue(1000);
  fetchSpy = spyOn(globalThis, 'fetch').mockImplementation(async (path) =>
    path === '/api/status'
      ? Response.json(makeStatus({ call_state: 'idle' }))
      : new Response(null, { status: 204 }),
  );
  const store = createIntercomStore();
  await store.startListen();

  // Act
  clockSpy.mockReturnValue(1000 + elapsed);
  await store.poll();

  // Assert
  expect(store.debugSessionActive()).toBe(active);
});

test('an active observation ends startup grace before its deadline', async () => {
  // Arrange
  clockSpy = spyOn(Date, 'now').mockReturnValue(1000);
  fetchSpy = spyOn(globalThis, 'fetch')
    .mockResolvedValueOnce(new Response(null, { status: 204 }))
    .mockResolvedValueOnce(Response.json(makeStatus({ call_state: 'idle' })))
    .mockResolvedValueOnce(Response.json(makeStatus({ call_state: 'listening' })))
    .mockResolvedValueOnce(Response.json(makeStatus({ call_state: 'idle' })));
  const store = createIntercomStore();
  await store.startListen();
  await store.poll();

  // Act
  clockSpy.mockReturnValue(1001);
  await store.poll();

  // Assert
  expect(store.debugSessionActive()).toBe(false);
});

test('a stale active observation cannot acknowledge a newer startup', async () => {
  // Arrange
  clockSpy = spyOn(Date, 'now').mockReturnValue(1000);
  const stale = Promise.withResolvers<Response>();
  fetchSpy = spyOn(globalThis, 'fetch')
    .mockReturnValueOnce(stale.promise)
    .mockResolvedValueOnce(new Response(null, { status: 204 }))
    .mockResolvedValueOnce(Response.json(makeStatus({ call_state: 'idle' })))
    .mockResolvedValueOnce(Response.json(makeStatus({ call_state: 'idle' })));
  const store = createIntercomStore();
  const oldPoll = store.poll();
  await store.startListen();

  // Act
  stale.resolve(Response.json(makeStatus({ call_state: 'listening' })));
  await oldPoll;
  await store.poll();

  // Assert
  expect(store.debugSessionActive()).toBe(true);
});

test('an older startup idle response cannot undo a newer active observation', async () => {
  // Arrange
  clockSpy = spyOn(Date, 'now').mockReturnValue(1000);
  const stale = Promise.withResolvers<Response>();
  fetchSpy = spyOn(globalThis, 'fetch')
    .mockResolvedValueOnce(new Response(null, { status: 204 }))
    .mockResolvedValueOnce(Response.json(makeStatus({ call_state: 'idle' })))
    .mockReturnValueOnce(stale.promise)
    .mockResolvedValueOnce(Response.json(makeStatus({ call_state: 'listening' })));
  const store = createIntercomStore();
  await store.startListen();
  const oldPoll = store.poll();
  await store.poll();

  // Act
  stale.resolve(Response.json(makeStatus({ call_state: 'idle' })));
  await oldPoll;

  // Assert
  expect(store.debugSessionActive()).toBe(true);
});

const audioActions = [
  [
    'setAudioGain',
    'updateAudioGain',
    [40, -12],
    '/api/action/audio/gain?rx_half_db=40&tx_half_db=-12',
    'Audio gain update failed',
  ],
  [
    'setBenchTone',
    'toggleBenchTone',
    [true],
    '/api/debug/audio/tone?enable=true',
    'Bench tone update failed',
  ],
] as const;

test.each(audioActions)(
  '%s posts audio calibration and refreshes store status',
  async (name, storeName, args, path) => {
    // Arrange
    const store = createIntercomStore();
    const action = Reflect.get(store, storeName);
    expect(Reflect.get(api, name)).toBeFunction();
    expect(action).toBeFunction();
    fetchSpy = spyOn(globalThis, 'fetch')
      .mockResolvedValueOnce(new Response(null, { status: 204 }))
      .mockResolvedValueOnce(
        Response.json(
          makeStatus({
            rx_gain_half_db: 40,
            tx_gain_half_db: -12,
            tone_active: true,
          }),
        ),
      );

    // Act
    await Reflect.apply(action, store, args);

    // Assert
    expect(fetchSpy).toHaveBeenNthCalledWith(1, path, { method: 'POST' });
    expect(fetchSpy).toHaveBeenNthCalledWith(2, '/api/status');
    expect(store.status()?.device.state.tone_active).toBe(true);
  },
);

test.each(audioActions)(
  '%s propagates audio action failures',
  async (name, _storeName, args, _path, message) => {
    // Arrange
    const action = Reflect.get(api, name);
    expect(action).toBeFunction();
    fetchSpy = spyOn(globalThis, 'fetch').mockResolvedValue(new Response(null, { status: 409 }));

    // Act
    const request = Reflect.apply(action, api, args);

    // Assert
    await expect(request).rejects.toThrow(`${message}: HTTP 409`);
  },
);

test('rapid independent gain edits preserve the optimistic pair and serialize POSTs', async () => {
  // Arrange
  const firstResponse = Promise.withResolvers<Response>();
  const secondResponse = Promise.withResolvers<Response>();
  const firstStarted = Promise.withResolvers<void>();
  const secondStarted = Promise.withResolvers<void>();
  let backendGain = { rx_gain_half_db: 40, tx_gain_half_db: 0 };
  const posts: string[] = [];
  fetchSpy = spyOn(globalThis, 'fetch').mockImplementation(async (path) => {
    if (path === '/api/status') return Response.json(makeStatus(backendGain));
    posts.push(String(path));
    if (posts.length === 1) {
      firstStarted.resolve();
      return firstResponse.promise;
    }
    secondStarted.resolve();
    return secondResponse.promise;
  });
  const store = createIntercomStore();
  const rxGain = Reflect.get(store, 'rxGainHalfDb') as () => number | undefined;
  const txGain = Reflect.get(store, 'txGainHalfDb') as () => number | undefined;
  expect(rxGain).toBeFunction();
  expect(txGain).toBeFunction();
  await store.poll();

  // Act
  const rxEdit = store.updateAudioGain(50, txGain() ?? 0);
  const txEdit = store.updateAudioGain(rxGain() ?? 40, -12);
  await firstStarted.promise;

  // Assert
  expect([rxGain(), txGain()]).toEqual([50, -12]);
  expect(posts).toEqual(['/api/action/audio/gain?rx_half_db=50&tx_half_db=0']);
  backendGain = { rx_gain_half_db: 50, tx_gain_half_db: 0 };
  firstResponse.resolve(new Response(null, { status: 204 }));
  await rxEdit;
  await secondStarted.promise;
  expect(posts).toEqual([
    '/api/action/audio/gain?rx_half_db=50&tx_half_db=0',
    '/api/action/audio/gain?rx_half_db=50&tx_half_db=-12',
  ]);
  expect([rxGain(), txGain()]).toEqual([50, -12]);
  secondResponse.resolve(new Response(null, { status: 204 }));
  await txEdit;
  // The REST endpoint queues a command; its first status response may still be old.
  expect([rxGain(), txGain()]).toEqual([50, -12]);
  backendGain = { rx_gain_half_db: 50, tx_gain_half_db: -12 };
  await store.poll();
  expect([rxGain(), txGain()]).toEqual([50, -12]);
});

test('a delayed older poll cannot undo a confirmed gain pair', async () => {
  // Arrange
  const stale = Promise.withResolvers<Response>();
  fetchSpy = spyOn(globalThis, 'fetch')
    .mockResolvedValueOnce(Response.json(makeStatus({ rx_gain_half_db: 40, tx_gain_half_db: 0 })))
    .mockReturnValueOnce(stale.promise)
    .mockResolvedValueOnce(new Response(null, { status: 204 }))
    .mockResolvedValueOnce(
      Response.json(makeStatus({ rx_gain_half_db: 50, tx_gain_half_db: -12 })),
    );
  const store = createIntercomStore();
  const rxGain = Reflect.get(store, 'rxGainHalfDb') as () => number | undefined;
  const txGain = Reflect.get(store, 'txGainHalfDb') as () => number | undefined;
  expect(rxGain).toBeFunction();
  expect(txGain).toBeFunction();
  await store.poll();
  const oldPoll = store.poll();
  await store.updateAudioGain(50, -12);

  // Act
  stale.resolve(Response.json(makeStatus({ rx_gain_half_db: 40, tx_gain_half_db: 0 })));
  await oldPoll;

  // Assert
  expect([rxGain(), txGain()]).toEqual([50, -12]);
});

test('a rejected gain write rolls back its optimistic pair without poisoning later writes', async () => {
  // Arrange
  fetchSpy = spyOn(globalThis, 'fetch')
    .mockResolvedValueOnce(Response.json(makeStatus({ rx_gain_half_db: 40, tx_gain_half_db: 0 })))
    .mockResolvedValueOnce(new Response(null, { status: 503 }))
    .mockResolvedValueOnce(new Response(null, { status: 204 }))
    .mockResolvedValueOnce(
      Response.json(makeStatus({ rx_gain_half_db: 60, tx_gain_half_db: -10 })),
    );
  const store = createIntercomStore();
  const rxGain = Reflect.get(store, 'rxGainHalfDb') as () => number | undefined;
  const txGain = Reflect.get(store, 'txGainHalfDb') as () => number | undefined;
  expect(rxGain).toBeFunction();
  expect(txGain).toBeFunction();
  await store.poll();

  // Act
  const rejected = store.updateAudioGain(50, -12);

  // Assert
  await expect(rejected).rejects.toThrow('Audio gain update failed: HTTP 503');
  expect([rxGain(), txGain()]).toEqual([40, 0]);
  await store.updateAudioGain(60, -10);
  expect([rxGain(), txGain()]).toEqual([60, -10]);
});

test('matching pre-write status cannot acknowledge a pair while earlier writes are queued', async () => {
  // Arrange
  const firstResponse = Promise.withResolvers<Response>();
  const secondResponse = Promise.withResolvers<Response>();
  const firstStarted = Promise.withResolvers<void>();
  let backendRx = 40;
  let posts = 0;
  fetchSpy = spyOn(globalThis, 'fetch').mockImplementation(async (path) => {
    if (path === '/api/status') {
      return Response.json(makeStatus({ rx_gain_half_db: backendRx, tx_gain_half_db: 0 }));
    }
    if (++posts === 1) {
      firstStarted.resolve();
      return firstResponse.promise;
    }
    return secondResponse.promise;
  });
  const store = createIntercomStore();
  await store.poll();
  const first = store.updateAudioGain(50, 0);
  const second = store.updateAudioGain(40, 0);
  await firstStarted.promise;

  // Act
  await store.poll();
  backendRx = 50;
  firstResponse.resolve(new Response(null, { status: 204 }));
  await first;

  // Assert
  expect(store.rxGainHalfDb()).toBe(40);
  backendRx = 40;
  secondResponse.resolve(new Response(null, { status: 204 }));
  await second;
  expect(store.rxGainHalfDb()).toBe(40);
});

test('failure of an older queued gain write cannot roll back the newer desired pair', async () => {
  // Arrange
  const firstResponse = Promise.withResolvers<Response>();
  let posts = 0;
  fetchSpy = spyOn(globalThis, 'fetch').mockImplementation(async (path) => {
    if (path === '/api/status') {
      return Response.json(makeStatus({ rx_gain_half_db: 40, tx_gain_half_db: 0 }));
    }
    return ++posts === 1 ? firstResponse.promise : new Response(null, { status: 204 });
  });
  const store = createIntercomStore();
  await store.poll();
  const first = store.updateAudioGain(50, 0);
  const second = store.updateAudioGain(store.rxGainHalfDb() ?? 40, -12);

  // Act
  firstResponse.resolve(new Response(null, { status: 503 }));
  await expect(first).rejects.toThrow('Audio gain update failed: HTTP 503');
  await second;

  // Assert
  expect(posts).toBe(2);
  expect([store.rxGainHalfDb(), store.txGainHalfDb()]).toEqual([50, -12]);
});

test('failed queued TX edit preserves a successful RX write through stale status and retry', async () => {
  // Arrange
  const firstResponse = Promise.withResolvers<Response>();
  const secondResponse = Promise.withResolvers<Response>();
  const posts: string[] = [];
  fetchSpy = spyOn(globalThis, 'fetch').mockImplementation(async (path) => {
    if (path === '/api/status') {
      return Response.json(makeStatus({ rx_gain_half_db: 40, tx_gain_half_db: 0 }));
    }
    posts.push(String(path));
    if (posts.length === 1) return firstResponse.promise;
    if (posts.length === 2) return secondResponse.promise;
    return new Response(null, { status: 204 });
  });
  const store = createIntercomStore();
  await store.poll();
  const rxEdit = store.updateAudioGain(50, 0);
  const txEdit = store.updateAudioGain(store.rxGainHalfDb() ?? 40, -12);

  // Act
  firstResponse.resolve(new Response(null, { status: 204 }));
  await rxEdit;
  secondResponse.resolve(new Response(null, { status: 503 }));
  await expect(txEdit).rejects.toThrow('Audio gain update failed: HTTP 503');

  // Assert
  expect([store.rxGainHalfDb(), store.txGainHalfDb()]).toEqual([50, 0]);
  await store.updateAudioGain(store.rxGainHalfDb() ?? 40, -12);
  expect(posts).toEqual([
    '/api/action/audio/gain?rx_half_db=50&tx_half_db=0',
    '/api/action/audio/gain?rx_half_db=50&tx_half_db=-12',
    '/api/action/audio/gain?rx_half_db=50&tx_half_db=-12',
  ]);
});

test('rollback after latest failure remains until confirmed then follows fresh status', async () => {
  // Arrange
  let backendRx = 40;
  let posts = 0;
  fetchSpy = spyOn(globalThis, 'fetch').mockImplementation(async (path) => {
    if (path === '/api/status') {
      return Response.json(makeStatus({ rx_gain_half_db: backendRx, tx_gain_half_db: 0 }));
    }
    return new Response(null, { status: ++posts === 1 ? 204 : 503 });
  });
  const store = createIntercomStore();
  await store.poll();
  await store.updateAudioGain(50, 0);
  await expect(store.updateAudioGain(50, -12)).rejects.toThrow(
    'Audio gain update failed: HTTP 503',
  );

  // Act
  await store.poll();

  // Assert
  expect([store.rxGainHalfDb(), store.txGainHalfDb()]).toEqual([50, 0]);
  backendRx = 50;
  await store.poll();
  backendRx = 55;
  await store.poll();
  expect(store.rxGainHalfDb()).toBe(55);
  await expect(store.updateAudioGain(60, -12)).rejects.toThrow(
    'Audio gain update failed: HTTP 503',
  );
  expect([store.rxGainHalfDb(), store.txGainHalfDb()]).toEqual([55, 0]);
});

test('dashboard API reads system, network, firmware, ecosystems, and device.state', async () => {
  // Arrange
  const payload = makeStatus({
    door_locked: false,
    door_unlock_duration_sec: 9,
    rx_gain_half_db: 52,
    tx_gain_half_db: -6,
  });
  fetchSpy = spyOn(globalThis, 'fetch').mockImplementation(async () => Response.json(payload));
  const store = createIntercomStore();

  // Act
  const fetched = await api.fetchStatus();
  await store.poll();

  // Assert
  expect(fetched).toEqual(payload);
  expect(store.status()?.system.status).toBe('Operational');
  expect(store.status()?.system.memory.internal).toEqual({
    free_bytes: 12480,
    total_bytes: 122880,
  });
  expect(store.status()?.system.cpu.core_0?.load_pct).toBe(25);
  expect(store.status()?.network.mac).toBe('98:A3:16:A6:78:44');
  expect(store.status()?.firmware.active_slot).toBe('ota_0');
  expect(store.status()?.ecosystems.available).toEqual(['matter']);
  expect(store.status()?.device.type).toBe('intercom_bridge');
  expect(store.status()?.device.state.door_locked).toBe(false);
  expect(store.doorUnlockDurationSec()).toBe(9);
  expect(store.rxGainHalfDb()).toBe(52);
  expect(store.txGainHalfDb()).toBe(-6);
});
