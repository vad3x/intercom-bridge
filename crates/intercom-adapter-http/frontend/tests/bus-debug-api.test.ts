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
});

let fetchSpy: ReturnType<typeof spyOn<typeof globalThis, 'fetch'>> | undefined;

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
