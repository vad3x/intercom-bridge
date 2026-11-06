import { decodeMulawToFloat32, encodeFloat32ToMulaw } from './mulaw';

const SAMPLE_RATE = 16000;
const FRAME_SAMPLES = 320;
const CTRL_HEARTBEAT = 0x00;
const CTRL_START_TALK = 0x01;
const CTRL_STOP_TALK = 0x02;
const MAX_BUFFERED_BYTES = FRAME_SAMPLES * 3;
const MAX_PLAYBACK_SECONDS = 0.12;
const TARGET_PLAYOUT_LEAD_SECONDS = 0.06;

export class WsIntercomClient {
  private ws: WebSocket | null = null;
  private audioCtx: AudioContext | null = null;
  private mediaStream: MediaStream | null = null;
  private source: MediaStreamAudioSourceNode | null = null;
  private processor: ScriptProcessorNode | null = null;
  private heartbeatTimer: ReturnType<typeof setInterval> | null = null;
  private playback = new Set<AudioBufferSourceNode>();
  private nextPlayTime = 0;
  private talking = false;
  private talkRequested = false;
  private talkGeneration = 0;

  constructor(private readonly callbacks: WsIntercomCallbacks) {}

  async connect(): Promise<void> {
    if (this.audioCtx) return;
    const context = new AudioContext({ sampleRate: SAMPLE_RATE });
    this.audioCtx = context;
    try {
      await context.resume();
      if (this.audioCtx !== context) return;
      if (context.sampleRate !== SAMPLE_RATE) {
        throw new Error('Live audio requires a browser supporting 16 kHz Web Audio');
      }
      this.nextPlayTime = context.currentTime;
      const proto = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
      const ws = new WebSocket(`${proto}//${window.location.host}/api/ws/audio`);
      ws.binaryType = 'arraybuffer';
      this.ws = ws;
      ws.onopen = () => {
        if (this.ws !== ws) return;
        this.callbacks.onConnectedChange(true);
        this.heartbeatTimer = setInterval(() => this.sendControl(CTRL_HEARTBEAT), 2000);
      };
      ws.onmessage = (event: MessageEvent<unknown>) => {
        if (this.ws !== ws || this.talking || !(event.data instanceof ArrayBuffer)) return;
        if (event.data.byteLength !== FRAME_SAMPLES) return;
        this.playFrame(context, new Uint8Array(event.data));
      };
      ws.onclose = (event: CloseEvent) => {
        if (this.ws !== ws) return;
        this.cleanup();
        if (event.code === 1013) this.callbacks.onPreempted();
      };
      ws.onerror = () => {
        if (this.ws !== ws) return;
        this.callbacks.onError('Live audio WebSocket connection failed');
        this.disconnect();
      };
    } catch (error: unknown) {
      if (this.audioCtx === context) this.cleanup();
      throw error;
    }
  }

  async startTalk(): Promise<void> {
    const ws = this.ws;
    const context = this.audioCtx;
    if (!ws || ws.readyState !== WebSocket.OPEN || !context || this.talkRequested) return;
    this.talkRequested = true;
    const generation = ++this.talkGeneration;
    let stream: MediaStream | null = null;
    try {
      if (!navigator.mediaDevices?.getUserMedia) {
        throw new Error('Microphone access requires HTTPS or localhost and a supported browser');
      }
      stream = await navigator.mediaDevices.getUserMedia({
        audio: { echoCancellation: true, noiseSuppression: true, channelCount: 1 },
      });
      if (this.ws !== ws || generation !== this.talkGeneration || !this.talkRequested) {
        for (const track of stream.getTracks()) track.stop();
        return;
      }
      this.mediaStream = stream;
      this.source = context.createMediaStreamSource(stream);
      // 32 ms blocks emit at most two 20 ms frames into the three-frame TX queue.
      const processor = context.createScriptProcessor(512, 1, 1);
      this.processor = processor;
      const frame = new Float32Array(FRAME_SAMPLES);
      let filled = 0;
      processor.onaudioprocess = (event: AudioProcessingEvent) => {
        if (!this.talking || this.ws !== ws || ws.readyState !== WebSocket.OPEN) return;
        const input = event.inputBuffer.getChannelData(0);
        for (const sample of input) {
          frame[filled++] = sample;
          if (filled === FRAME_SAMPLES) {
            // Bound latency when the network stalls instead of queuing stale speech.
            if (ws.bufferedAmount < MAX_BUFFERED_BYTES) {
              ws.send(encodeFloat32ToMulaw(frame));
              this.callbacks.onTxLevelChange?.(rmsDbfs(frame));
            }
            filled = 0;
          }
        }
      };
      this.source.connect(processor);
      processor.connect(context.destination);
      this.clearPlayback();
      this.sendControl(CTRL_START_TALK);
      this.talking = true;
      this.callbacks.onTalkingChange?.(true);
    } catch (error: unknown) {
      if (generation !== this.talkGeneration) return;
      this.stopTalk();
      throw error;
    }
  }

  stopTalk(): void {
    this.talkRequested = false;
    this.talkGeneration += 1;
    if (this.talking) this.sendControl(CTRL_STOP_TALK);
    this.talking = false;
    this.callbacks.onTalkingChange?.(false);
    this.callbacks.onTxLevelChange?.(-96);
    if (this.processor) {
      this.processor.onaudioprocess = null;
      this.processor.disconnect();
      this.processor = null;
    }
    this.source?.disconnect();
    this.source = null;
    if (this.mediaStream) {
      for (const track of this.mediaStream.getTracks()) track.stop();
      this.mediaStream = null;
    }
    this.nextPlayTime = this.audioCtx?.currentTime ?? 0;
  }

  disconnect(): void {
    const ws = this.ws;
    this.cleanup();
    ws?.close();
  }

  private sendControl(byte: number): void {
    if (this.ws?.readyState === WebSocket.OPEN) this.ws.send(new Uint8Array([byte]));
  }

  private playFrame(context: AudioContext, bytes: Uint8Array): void {
    const now = context.currentTime;
    if (this.nextPlayTime > now + MAX_PLAYBACK_SECONDS) return;
    const pcm = decodeMulawToFloat32(bytes);
    const buffer = context.createBuffer(1, pcm.length, SAMPLE_RATE);
    buffer.getChannelData(0).set(pcm);
    const source = context.createBufferSource();
    source.buffer = buffer;
    source.connect(context.destination);
    source.onended = () => {
      source.disconnect();
      this.playback.delete(source);
    };
    this.playback.add(source);
    this.nextPlayTime = Math.max(this.nextPlayTime, now + TARGET_PLAYOUT_LEAD_SECONDS);
    source.start(this.nextPlayTime);
    this.nextPlayTime += pcm.length / SAMPLE_RATE;
  }

  private clearPlayback(): void {
    for (const source of this.playback) {
      source.onended = null;
      source.stop();
      source.disconnect();
    }
    this.playback.clear();
  }

  private cleanup(): void {
    this.stopTalk();
    if (this.heartbeatTimer !== null) {
      clearInterval(this.heartbeatTimer);
      this.heartbeatTimer = null;
    }
    this.clearPlayback();
    if (this.audioCtx) {
      void this.audioCtx.close().catch(() => {});
      this.audioCtx = null;
    }
    if (this.ws) {
      this.ws.onopen = null;
      this.ws.onmessage = null;
      this.ws.onclose = null;
      this.ws.onerror = null;
      this.ws = null;
    }
    this.callbacks.onConnectedChange(false);
  }
}

export interface WsIntercomCallbacks {
  onConnectedChange: (connected: boolean) => void;
  onTalkingChange?: (talking: boolean) => void;
  onTxLevelChange?: (dbfs: number) => void;
  onPreempted: () => void;
  onError: (message: string) => void;
}

function rmsDbfs(samples: Float32Array): number {
  let sum = 0;
  for (const sample of samples) sum += sample * sample;
  return Math.max(-96, Math.min(0, Math.round(10 * Math.log10(sum / samples.length))));
}
