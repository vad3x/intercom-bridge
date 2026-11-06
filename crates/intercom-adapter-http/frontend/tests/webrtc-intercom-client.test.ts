import { afterEach, beforeEach, expect, mock, test } from 'bun:test';
import { WebRtcIntercomClient } from '../src/audio/webrtcIntercomClient';

class Sender {
  replaceTrack = mock(async (_track: MediaStreamTrack | null) => {});
}

class PeerConnection {
  static instances: PeerConnection[] = [];
  connectionState: RTCPeerConnectionState = 'new';
  iceGatheringState: RTCIceGatheringState = 'complete';
  localDescription: RTCSessionDescriptionInit | null = null;
  remoteDescription: RTCSessionDescriptionInit | null = null;
  ontrack: ((event: { streams: MediaStream[]; track: MediaStreamTrack }) => void) | null = null;
  onconnectionstatechange: (() => void) | null = null;
  sender = new Sender();
  transceiver = {
    sender: this.sender,
    setCodecPreferences: mock((_codecs: RTCRtpCodec[]) => {}),
  };
  addTransceiver = mock((_kind: string, _init: RTCRtpTransceiverInit) => this.transceiver);
  createOffer = mock(
    async () => ({ type: 'offer', sdp: 'offer-sdp' }) as RTCSessionDescriptionInit,
  );
  setLocalDescription = mock(async (description: RTCSessionDescriptionInit) => {
    this.localDescription = description;
  });
  setRemoteDescription = mock(async (description: RTCSessionDescriptionInit) => {
    this.remoteDescription = description;
  });
  close = mock(() => {
    this.connectionState = 'closed';
    this.onconnectionstatechange?.();
  });
  constructor() {
    PeerConnection.instances.push(this);
  }
}

class AudioElement {
  static instances: AudioElement[] = [];
  autoplay = false;
  setAttribute = mock((_name: string, _value: string) => {});
  srcObject: MediaStream | null = null;
  play = mock(async () => {});
  pause = mock(() => {});
  constructor() {
    AudioElement.instances.push(this);
  }
}

const receiver = {
  getCapabilities: mock(() => ({
    codecs: [{ mimeType: 'audio/opus' }, { mimeType: 'audio/PCMU' }],
  })),
};

const saved = new Map<string, PropertyDescriptor | undefined>();
let fetchMock: ReturnType<typeof mock>;
let microphone: ReturnType<typeof mock>;
let microphoneTrack: { stop: ReturnType<typeof mock> };
let microphoneStream: MediaStream;

beforeEach(() => {
  PeerConnection.instances = [];
  AudioElement.instances = [];
  fetchMock = mock(async () => ({ ok: true, text: async () => 'answer-sdp' }) as Response);
  microphoneTrack = { stop: mock(() => {}) };
  microphoneStream = {
    getTracks: () => [microphoneTrack],
    getAudioTracks: () => [microphoneTrack],
  } as unknown as MediaStream;
  microphone = mock(async () => microphoneStream);
  for (const [name, value] of Object.entries({
    RTCPeerConnection: PeerConnection,
    RTCRtpReceiver: receiver,
    Audio: AudioElement,
    fetch: fetchMock,
    window: { location: { origin: 'http://intercom.local:8080' } },
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
    onError: mock((_message: string) => {}),
    onTalkingChange: mock((_value: boolean) => {}),
    onTxLevelChange: mock((_value: number) => {}),
  };
  return { client: new WebRtcIntercomClient(callbacks), callbacks };
}

test('negotiates Opus with the shared media engine and plays its remote audio track', async () => {
  const { client, callbacks } = setup();

  await client.connect();
  const peer = PeerConnection.instances[0];
  const request = fetchMock.mock.calls[0] as unknown as [string, RequestInit];
  const offeredCodecs = peer.transceiver.setCodecPreferences.mock.calls[0]?.[0];
  const remoteStream = {} as MediaStream;

  expect(peer.addTransceiver).toHaveBeenCalledWith('audio', { direction: 'sendrecv' });
  expect(offeredCodecs).toEqual([{ mimeType: 'audio/opus' }]);
  expect(request[0]).toBe('/api/debug/audio/webrtc/offer');
  expect(request[1].body).toBe('offer-sdp');
  expect(peer.remoteDescription).toEqual({ type: 'answer', sdp: 'answer-sdp' });

  peer.connectionState = 'connected';
  peer.onconnectionstatechange?.();
  peer.ontrack?.({ streams: [remoteStream], track: {} as MediaStreamTrack });

  expect(callbacks.onConnectedChange).toHaveBeenLastCalledWith(true);
  expect(AudioElement.instances[0].srcObject).toBe(remoteStream);
  expect(AudioElement.instances[0].play).toHaveBeenCalledTimes(1);
  client.disconnect();
  expect(PeerConnection.instances[0].close).toHaveBeenCalledTimes(1);
});

test('routes push-to-talk microphone tracks through the negotiated WebRTC sender', async () => {
  const { client, callbacks } = setup();
  await client.connect();
  const peer = PeerConnection.instances[0];
  peer.connectionState = 'connected';

  await client.startTalk();
  client.stopTalk();

  expect(peer.sender.replaceTrack).toHaveBeenNthCalledWith(1, microphoneTrack);
  expect(peer.sender.replaceTrack).toHaveBeenNthCalledWith(2, null);
  expect(microphoneTrack.stop).toHaveBeenCalledTimes(1);
  expect(callbacks.onTalkingChange).toHaveBeenNthCalledWith(1, true);
  expect(callbacks.onTalkingChange).toHaveBeenLastCalledWith(false);
  client.disconnect();
});

test('a rejected second offer does not send stop for the active Debug session', async () => {
  const requests: string[] = [];
  fetchMock = mock(async (input: RequestInfo | URL) => {
    requests.push(String(input));
    return {
      ok: false,
      status: 409,
      text: async () => 'WebRTC media session busy',
    } as Response;
  });
  Object.defineProperty(globalThis, 'fetch', { configurable: true, value: fetchMock });
  const { client } = setup();

  await expect(client.connect()).rejects.toThrow('WebRTC media session busy');

  expect(requests).toEqual(['/api/debug/audio/webrtc/offer']);
});
