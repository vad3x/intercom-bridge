const OFFER_PATH = '/api/debug/audio/webrtc/offer';
const STOP_PATH = '/api/debug/audio/webrtc/stop';
const ICE_GATHER_TIMEOUT_MS = 8_000;
const TX_STATS_INTERVAL_MS = 250;

type AudioEnergyStats = RTCStats & {
  type: string;
  kind?: string;
  mediaType?: string;
  totalAudioEnergy?: number;
  totalSamplesDuration?: number;
};

export class WebRtcIntercomClient {
  private peer: RTCPeerConnection | null = null;
  private acceptedPeer: RTCPeerConnection | null = null;
  private sender: RTCRtpSender | null = null;
  private localStream: MediaStream | null = null;
  private remoteAudio: HTMLAudioElement | null = null;
  private txStatsTimer: ReturnType<typeof setInterval> | null = null;
  private previousTxEnergy: { energy: number; duration: number } | null = null;
  private talking = false;
  private talkRequested = false;
  private talkGeneration = 0;

  constructor(private readonly callbacks: WebRtcIntercomCallbacks) {}

  async connect(): Promise<void> {
    if (this.peer) return;
    const peer = new RTCPeerConnection({ iceServers: [] });
    this.peer = peer;
    peer.ontrack = (event) => this.playRemoteTrack(peer, event);
    peer.onconnectionstatechange = () => {
      if (this.peer !== peer) return;
      if (peer.connectionState === 'connected') {
        this.callbacks.onConnectedChange(true);
      } else if (peer.connectionState === 'failed') {
        this.callbacks.onError('Debug WebRTC connection failed');
        this.disconnect();
      }
    };

    try {
      const transceiver = peer.addTransceiver('audio', { direction: 'sendrecv' });
      this.sender = transceiver.sender;
      const opus = RTCRtpReceiver.getCapabilities('audio')?.codecs.filter(
        (codec) => codec.mimeType.toLowerCase() === 'audio/opus',
      );
      if (opus?.length) transceiver.setCodecPreferences(opus);

      const offer = await peer.createOffer();
      await peer.setLocalDescription(offer);
      await waitForIceGathering(peer);
      if (this.peer !== peer) return;
      const sdp = peer.localDescription?.sdp;
      if (!sdp) throw new Error('Browser did not produce a WebRTC SDP offer');

      const response = await fetch(OFFER_PATH, {
        method: 'POST',
        headers: { 'content-type': 'application/sdp' },
        body: sdp,
      });
      if (!response.ok) {
        const detail = await response.text();
        throw new Error(detail || `WebRTC signaling failed (${response.status})`);
      }
      this.acceptedPeer = peer;
      const answer = await response.text();
      if (this.peer !== peer) {
        this.stopAcceptedSession(peer);
        return;
      }
      await peer.setRemoteDescription({ type: 'answer', sdp: answer });
    } catch (error: unknown) {
      if (this.peer === peer) this.disconnect();
      throw error;
    }
  }

  async startTalk(): Promise<void> {
    const peer = this.peer;
    const sender = this.sender;
    if (!peer || !sender || peer.connectionState !== 'connected' || this.talkRequested) return;
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
      if (!this.isCurrentTalk(peer, generation)) {
        stopTracks(stream);
        return;
      }
      const track = stream.getAudioTracks()[0];
      if (!track) throw new Error('No microphone audio track was captured');
      await sender.replaceTrack(track);
      if (!this.isCurrentTalk(peer, generation)) {
        await sender.replaceTrack(null);
        stopTracks(stream);
        return;
      }
      this.localStream = stream;
      this.talking = true;
      this.startTxMeter(peer, sender);
      this.callbacks.onTalkingChange?.(true);
    } catch (error: unknown) {
      if (generation === this.talkGeneration) this.stopTalk();
      throw error;
    }
  }

  stopTalk(): void {
    this.talkRequested = false;
    this.talkGeneration += 1;
    if (this.talking) void this.sender?.replaceTrack(null).catch(() => {});
    this.talking = false;
    this.stopTxMeter();
    this.callbacks.onTalkingChange?.(false);
    this.callbacks.onTxLevelChange?.(-96);
    if (this.localStream) {
      stopTracks(this.localStream);
      this.localStream = null;
    }
  }

  disconnect(): void {
    const peer = this.peer;
    this.stopTalk();
    this.peer = null;
    this.sender = null;
    if (peer) {
      peer.ontrack = null;
      peer.onconnectionstatechange = null;
      peer.close();
      this.stopAcceptedSession(peer);
    }
    if (this.remoteAudio) {
      this.remoteAudio.pause();
      this.remoteAudio.srcObject = null;
      this.remoteAudio = null;
    }
    this.callbacks.onConnectedChange(false);
  }

  private stopAcceptedSession(peer: RTCPeerConnection): void {
    if (this.acceptedPeer !== peer) return;
    this.acceptedPeer = null;
    void fetch(STOP_PATH, { method: 'POST', keepalive: true }).catch(() => {});
  }

  private isCurrentTalk(peer: RTCPeerConnection, generation: number): boolean {
    return this.peer === peer && this.talkGeneration === generation && this.talkRequested;
  }

  private playRemoteTrack(peer: RTCPeerConnection, event: RTCTrackEvent): void {
    if (this.peer !== peer) return;
    const audio = new Audio();
    audio.autoplay = true;
    audio.setAttribute('playsinline', '');
    audio.srcObject = event.streams[0] ?? new MediaStream([event.track]);
    this.remoteAudio = audio;
    void audio.play().catch(() => {
      this.callbacks.onError('Browser blocked audio playback; reconnect after enabling audio');
    });
  }

  private startTxMeter(peer: RTCPeerConnection, sender: RTCRtpSender): void {
    this.stopTxMeter();
    this.previousTxEnergy = null;
    this.txStatsTimer = setInterval(() => {
      void peer
        .getStats(sender.track ?? undefined)
        .then((stats) => {
          if (this.peer !== peer || !this.talking) return;
          let audioStats: AudioEnergyStats | undefined;
          stats.forEach((report) => {
            const candidate = report as AudioEnergyStats;
            if (
              candidate.type === 'outbound-rtp' &&
              (candidate.kind === 'audio' || candidate.mediaType === 'audio') &&
              candidate.totalAudioEnergy !== undefined &&
              candidate.totalSamplesDuration !== undefined
            ) {
              audioStats = candidate;
            }
          });
          if (!audioStats) return;
          const current = {
            energy: audioStats.totalAudioEnergy as number,
            duration: audioStats.totalSamplesDuration as number,
          };
          const previous = this.previousTxEnergy;
          this.previousTxEnergy = current;
          if (previous && current.duration > previous.duration) {
            const meanSquare =
              (current.energy - previous.energy) / (current.duration - previous.duration);
            const dbfs = meanSquare > 0 ? 10 * Math.log10(meanSquare) : -96;
            this.callbacks.onTxLevelChange?.(Math.max(-96, Math.min(0, Math.round(dbfs))));
          }
        })
        .catch(() => {});
    }, TX_STATS_INTERVAL_MS);
  }

  private stopTxMeter(): void {
    if (this.txStatsTimer !== null) {
      clearInterval(this.txStatsTimer);
      this.txStatsTimer = null;
    }
    this.previousTxEnergy = null;
  }
}

export interface WebRtcIntercomCallbacks {
  onConnectedChange: (connected: boolean) => void;
  onTalkingChange?: (talking: boolean) => void;
  onTxLevelChange?: (dbfs: number) => void;
  onError: (message: string) => void;
}

function stopTracks(stream: MediaStream): void {
  for (const track of stream.getTracks()) track.stop();
}

function waitForIceGathering(peer: RTCPeerConnection): Promise<void> {
  if (peer.iceGatheringState === 'complete') return Promise.resolve();
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => {
      peer.removeEventListener('icegatheringstatechange', onStateChange);
      reject(new Error('Browser ICE gathering timed out'));
    }, ICE_GATHER_TIMEOUT_MS);
    const onStateChange = () => {
      if (peer.iceGatheringState !== 'complete') return;
      clearTimeout(timeout);
      peer.removeEventListener('icegatheringstatechange', onStateChange);
      resolve();
    };
    peer.addEventListener('icegatheringstatechange', onStateChange);
  });
}
