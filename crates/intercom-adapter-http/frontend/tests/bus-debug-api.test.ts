import { afterEach, expect, mock, spyOn, test } from 'bun:test';

// The shared barrel also loads browser-only UI; these helpers only need fetch.
mock.module('@embedded-device-foundation/dashboard-core', () => ({
  createDashboardApi: () => ({}),
}));
const api = await import('../src/api');

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
  const action = Reflect.get(api, name) as (() => Promise<void>) | undefined;
  expect(action).toBeFunction();
  fetchSpy = spyOn(globalThis, 'fetch').mockResolvedValue(new Response(null, { status: 204 }));

  await action?.();

  expect(fetchSpy).toHaveBeenCalledTimes(1);
  expect(fetchSpy).toHaveBeenCalledWith(path, { method: 'POST' });
});

test.each(actions)('%s rejects non-OK responses', async (name, _path, message) => {
  const action = Reflect.get(api, name) as (() => Promise<void>) | undefined;
  expect(action).toBeFunction();
  fetchSpy = spyOn(globalThis, 'fetch').mockResolvedValue(new Response(null, { status: 503 }));

  await expect(action?.()).rejects.toThrow(`${message}: HTTP 503`);
});
