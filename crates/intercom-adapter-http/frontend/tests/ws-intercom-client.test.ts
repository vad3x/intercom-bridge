import { afterEach, beforeEach, expect, mock, test } from 'bun:test';
import { WsIntercomClient } from '../src/audio/wsIntercomClient';

class Socket {
  static OPEN = 1;
  static instances: Socket[] = [];
  readyState = 0;
  binaryType = '';
  bufferedAmount = 0;
  onopen: (() => void) | null = null;
  onclose: ((event: { code: number }) => void) | null = null;
  onerror: (() => void) | null = null;
  onmessage: ((event: { data: ArrayBuffer }) => void) | null = null;
  send = mock((_data: Uint8Array) => {});
  close = mock(() => {});
  constructor(public url: string) {
    Socket.instances.push(this);
  }
  open() {
    this.readyState = 1;
    this.onopen?.();
  }
}

class Context {
  static instances: Context[] = [];
  static rate = 16000;
  sampleRate = Context.rate;
  currentTime = 1;
  destination = {};
  processor = {
    connect: mock(() => {}),
    disconnect: mock(() => {}),
    onaudioprocess: null as
      | ((event: { inputBuffer: { getChannelData: () => Float32Array } }) => void)
      | null,
  };
  source = { connect: mock(() => {}), disconnect: mock(() => {}) };
  playback = {
    buffer: null,
    connect: mock(() => {}),
    disconnect: mock(() => {}),
    start: mock(() => {}),
    stop: mock(() => {}),
    onended: null,
  };
  resume = mock(async () => {});
  close = mock(async () => {});
  createMediaStreamSource = mock(() => this.source);
  createScriptProcessor = mock(() => this.processor);
  createBuffer = mock((_channels: number, length: number) => ({
    getChannelData: () => new Float32Array(length),
  }));
  createBufferSource = mock(() => ({ ...this.playback }));
  constructor() {
    Context.instances.push(this);
  }
}

const saved = new Map<string, PropertyDescriptor | undefined>();
const stopTrack = mock(() => {});
const stream = { getTracks: () => [{ stop: stopTrack }] };
let microphone = mock(async () => stream);

beforeEach(() => {
  Context.rate = 16000;
  Socket.instances = [];
  Context.instances = [];
  stopTrack.mockClear();
  microphone = mock(async () => stream);
  for (const [name, value] of Object.entries({
    WebSocket: Socket,
    AudioContext: Context,
    window: { location: { protocol: 'https:', host: 'intercom.local:8080' } },
    navigator: {
      mediaDevices: { getUserMedia: (...args: unknown[]) => Reflect.apply(microphone, null, args) },
    },
  })) {
    saved.set(name, Object.getOwnPropertyDescriptor(globalThis, name));
    Object.defineProperty(globalThis, name, { configurable: true, value });
  }
});
afterEach(() => {
  for (const [name, descriptor] of saved) {
    if (descriptor) Object.defineProperty(globalThis, name, descriptor);
    else Reflect.deleteProperty(globalThis, name);
  }
});

function setup() {
  const callbacks = {
    onConnectedChange: mock((_value: boolean) => {}),
    onPreempted: mock(() => {}),
    onError: mock((_message: string) => {}),
    onTalkingChange: mock((_value: boolean) => {}),
    onTxLevelChange: mock((_value: number) => {}),
  };
  return { client: new WsIntercomClient(callbacks), callbacks };
}
async function connect(client: WsIntercomClient) {
  await client.connect();
  const socket = Socket.instances.at(-1) as Socket;
  socket.open();
  return { socket, context: Context.instances.at(-1) as Context };
}

test('connects once, schedules valid downlink and closes on preemption', async () => {
  // Arrange
  const { client, callbacks } = setup();
  const { socket, context } = await connect(client);

  // Act
  await client.connect();
  socket.onmessage?.({ data: new Uint8Array(320).fill(255).buffer });
  socket.onmessage?.({ data: new Uint8Array(1).buffer });
  socket.onclose?.({ code: 1013 });

  // Assert
  expect(Socket.instances).toHaveLength(1);
  expect(socket.url).toBe('wss://intercom.local:8080/api/ws/audio');
  expect(context.createBuffer).toHaveBeenCalledTimes(1);
  expect(context.close).toHaveBeenCalledTimes(1);
  expect(callbacks.onPreempted).toHaveBeenCalledTimes(1);
  expect(callbacks.onConnectedChange).toHaveBeenLastCalledWith(false);
});

test('PTT sends control and exact 320-byte frames, drops partial audio on release', async () => {
  // Arrange
  const { client } = setup();
  const { socket, context } = await connect(client);
  const process = () =>
    context.processor.onaudioprocess?.({
      inputBuffer: { getChannelData: () => new Float32Array(1024) },
    });

  // Act
  await client.startTalk();
  process();
  client.stopTalk();
  await client.startTalk();
  process();
  client.stopTalk();
  client.disconnect();

  // Assert
  expect(socket.send.mock.calls.map(([bytes]) => bytes.length)).toEqual([
    1, 320, 320, 320, 1, 1, 320, 320, 320, 1,
  ]);
  expect([...socket.send.mock.calls[0][0]]).toEqual([1]);
  expect([...socket.send.mock.calls[4][0]]).toEqual([2]);
  expect(stopTrack).toHaveBeenCalled();
});

test.each(['release', 'disconnect'] as const)(
  '%s during microphone prompt cannot begin delayed transmission',
  async (action) => {
    // Arrange
    const { client, callbacks } = setup();
    const { socket, context } = await connect(client);
    const pending = Promise.withResolvers<typeof stream>();
    microphone = mock(() => pending.promise);
    const start = client.startTalk();

    // Act
    if (action === 'release') client.stopTalk();
    else client.disconnect();
    pending.resolve(stream);
    await start;
    client.disconnect();

    // Assert
    expect(socket.send).not.toHaveBeenCalled();
    expect(context.createMediaStreamSource).not.toHaveBeenCalled();
    expect(stopTrack).toHaveBeenCalled();
    expect(callbacks.onTalkingChange).not.toHaveBeenCalledWith(true);
  },
);

test('old socket close cannot tear down a replacement connection', async () => {
  // Arrange
  const { client, callbacks } = setup();
  const old = await connect(client);
  const closeOld = old.socket.onclose;
  client.disconnect();
  const current = await connect(client);

  // Act
  closeOld?.({ code: 1013 });

  // Assert
  expect(current.context.close).not.toHaveBeenCalled();
  expect(callbacks.onConnectedChange).toHaveBeenLastCalledWith(true);
  expect(callbacks.onPreempted).not.toHaveBeenCalled();
  client.disconnect();
});

test('unsupported sample rate fails before opening a socket', async () => {
  // Arrange
  const { client } = setup();
  Context.rate = 48000;

  // Act
  const request = client.connect();

  // Assert
  await expect(request).rejects.toThrow('16 kHz');
  expect(Socket.instances).toHaveLength(0);
  client.disconnect();
});

test('TX meter reports outgoing frame RMS and resets on release', async () => {
  // Arrange
  const { client, callbacks } = setup();
  const { context } = await connect(client);
  await client.startTalk();

  // Act
  context.processor.onaudioprocess?.({
    inputBuffer: { getChannelData: () => new Float32Array(320).fill(0.5) },
  });

  // Assert
  expect(callbacks.onTxLevelChange).toHaveBeenLastCalledWith(-6);
  client.stopTalk();
  expect(callbacks.onTxLevelChange).toHaveBeenLastCalledWith(-96);
  client.disconnect();
});

test('microphone rejection leaves PTT stopped and permits a later retry', async () => {
  // Arrange
  const { client, callbacks } = setup();
  const { socket } = await connect(client);
  microphone = mock(async () => {
    throw new Error('Permission denied');
  });

  // Act
  const rejected = client.startTalk();

  // Assert
  await expect(rejected).rejects.toThrow('Permission denied');
  expect(socket.send).not.toHaveBeenCalled();
  expect(callbacks.onTalkingChange).toHaveBeenLastCalledWith(false);
  microphone = mock(async () => stream);
  await client.startTalk();
  expect(callbacks.onTalkingChange).toHaveBeenLastCalledWith(true);
  client.disconnect();
});

test('WebSocket error releases audio resources and resets connection state', async () => {
  // Arrange
  const { client, callbacks } = setup();
  const { socket, context } = await connect(client);
  await client.startTalk();

  // Act
  socket.onerror?.();

  // Assert
  expect(callbacks.onError).toHaveBeenCalledTimes(1);
  expect(callbacks.onConnectedChange).toHaveBeenLastCalledWith(false);
  expect(callbacks.onTalkingChange).toHaveBeenLastCalledWith(false);
  expect(context.close).toHaveBeenCalledTimes(1);
  expect(stopTrack).toHaveBeenCalledTimes(1);
  expect(socket.close).toHaveBeenCalledTimes(1);
});

test('TX drops stalled frames and a new press never sends the previous partial frame', async () => {
  // Arrange
  const { client } = setup();
  const { socket, context } = await connect(client);
  await client.startTalk();
  const process = (size: number) =>
    context.processor.onaudioprocess?.({
      inputBuffer: { getChannelData: () => new Float32Array(size) },
    });

  // Act
  process(319);
  client.stopTalk();
  await client.startTalk();
  process(1);
  socket.bufferedAmount = 960;
  process(319);
  socket.bufferedAmount = 0;
  process(320);
  client.disconnect();

  // Assert
  expect(socket.send.mock.calls.map(([bytes]) => bytes.length)).toEqual([1, 1, 1, 320, 1]);
});

test('capture blocks keep each outgoing burst within the three-frame device queue', async () => {
  // Arrange
  const { client } = setup();
  const { socket, context } = await connect(client);
  await client.startTalk();
  const blockSize = Reflect.get(context.createScriptProcessor.mock.calls[0], '0') as number;
  const burstSizes: number[] = [];

  // Act
  for (let block = 0; block < 10; block += 1) {
    const before = socket.send.mock.calls.length;
    context.processor.onaudioprocess?.({
      inputBuffer: { getChannelData: () => new Float32Array(blockSize) },
    });
    burstSizes.push(socket.send.mock.calls.length - before);
  }
  client.disconnect();

  // Assert
  expect(Math.max(...burstSizes)).toBeLessThanOrEqual(2);
  expect(burstSizes.reduce((sum, size) => sum + size, 0)).toBe(Math.floor((blockSize * 10) / 320));
});
