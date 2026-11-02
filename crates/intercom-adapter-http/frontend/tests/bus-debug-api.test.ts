import { afterEach, expect, mock, spyOn, test } from 'bun:test';

// The shared barrel also loads browser-only UI; these helpers only need fetch.
mock.module('@embedded-device-foundation/dashboard-core', () => ({
  createDashboardApi: () => ({
    fetchStatus: async () => (await fetch('/api/status')).json(),
  }),
}));
const api = await import('../src/api');
const { createIntercomStore } = await import('../src/store/intercomStore');

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
        ? Response.json({ door_locked: true, call_state: 'idle' })
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
    .mockResolvedValueOnce(Response.json({ door_locked: true, call_state: 'listening' }));
  const store = createIntercomStore();
  const oldPoll = store.poll();
  await store.startListen();

  // Act
  stale.resolve(Response.json({ door_locked: true, call_state: 'idle' }));
  await oldPoll;

  // Assert
  expect(store.debugSessionActive()).toBe(true);
});

test('idle from a superseded session cannot end a newer session', async () => {
  // Arrange
  const stale = Promise.withResolvers<Response>();
  fetchSpy = spyOn(globalThis, 'fetch').mockImplementation(async (path) =>
    path === '/api/status'
      ? Response.json({ door_locked: true, call_state: 'listening' })
      : new Response(null, { status: 204 }),
  );
  const store = createIntercomStore();
  await store.startListen();
  fetchSpy.mockReturnValueOnce(stale.promise);
  const oldPoll = store.poll();
  await store.startTalk();

  // Act
  stale.resolve(Response.json({ door_locked: true, call_state: 'idle' }));
  await oldPoll;

  // Assert
  expect(store.debugSessionActive()).toBe(true);
});

test('a later current-session idle observation ends the heartbeat session', async () => {
  // Arrange
  fetchSpy = spyOn(globalThis, 'fetch')
    .mockResolvedValueOnce(new Response(null, { status: 204 }))
    .mockResolvedValueOnce(Response.json({ door_locked: true, call_state: 'listening' }))
    .mockResolvedValueOnce(Response.json({ door_locked: true, call_state: 'idle' }));
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
    .mockResolvedValueOnce(Response.json({ door_locked: true, call_state: 'listening' }))
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
        ? Response.json({ door_locked: true, call_state: 'idle' })
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
      ? Response.json({ door_locked: true, call_state: 'idle' })
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
    .mockResolvedValueOnce(Response.json({ door_locked: true, call_state: 'idle' }))
    .mockResolvedValueOnce(Response.json({ door_locked: true, call_state: 'listening' }))
    .mockResolvedValueOnce(Response.json({ door_locked: true, call_state: 'idle' }));
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
    .mockResolvedValueOnce(Response.json({ door_locked: true, call_state: 'idle' }))
    .mockResolvedValueOnce(Response.json({ door_locked: true, call_state: 'idle' }));
  const store = createIntercomStore();
  const oldPoll = store.poll();
  await store.startListen();

  // Act
  stale.resolve(Response.json({ door_locked: true, call_state: 'listening' }));
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
    .mockResolvedValueOnce(Response.json({ door_locked: true, call_state: 'idle' }))
    .mockReturnValueOnce(stale.promise)
    .mockResolvedValueOnce(Response.json({ door_locked: true, call_state: 'listening' }));
  const store = createIntercomStore();
  await store.startListen();
  const oldPoll = store.poll();
  await store.poll();

  // Act
  stale.resolve(Response.json({ door_locked: true, call_state: 'idle' }));
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
        Response.json({
          door_locked: true,
          rx_gain_half_db: 40,
          tx_gain_half_db: -12,
          tone_active: true,
        }),
      );

    // Act
    await Reflect.apply(action, store, args);

    // Assert
    expect(fetchSpy).toHaveBeenNthCalledWith(1, path, { method: 'POST' });
    expect(fetchSpy).toHaveBeenNthCalledWith(2, '/api/status');
    expect(store.status()?.tone_active).toBe(true);
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
