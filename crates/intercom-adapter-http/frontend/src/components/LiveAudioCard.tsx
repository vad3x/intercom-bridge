import { Badge, Card, CardBody, CardTitle } from '@embedded-device-foundation/dashboard-core';
import { Mic, PhoneOff, Volume2 } from 'lucide-solid';
import { type Component, createSignal, onCleanup } from 'solid-js';
import { WebRtcIntercomClient } from '../audio/webrtcIntercomClient';
import type { BusLineState, CallState } from '../types';

export interface LiveAudioCardProps {
  busState: BusLineState | undefined;
  callState: CallState | undefined;
  rxRmsDbfs: number | undefined;
  rxPeakDbfs: number | undefined;
  rxGainHalfDb: number | undefined;
  txGainHalfDb: number | undefined;
  onStartListen: () => Promise<void> | void;
  onStartTalk: () => Promise<void> | void;
  onStopTalk: () => Promise<void> | void;
  onHangup: () => Promise<void> | void;
  onGainChange: (rxHalfDb: number, txHalfDb: number) => Promise<void> | void;
  onNotify: (msg: string, kind: 'info' | 'success' | 'error') => void;
}

export const LiveAudioCard: Component<LiveAudioCardProps> = (props) => {
  const [transport, setTransport] = createSignal<'webrtc' | 'relay'>('webrtc');
  const [connected, setConnected] = createSignal(false);
  const [talking, setTalking] = createSignal(false);
  const [connecting, setConnecting] = createSignal(false);
  const [pendingTalk, setPendingTalk] = createSignal(false);
  const [txRms, setTxRms] = createSignal(-96);

  const runAction = async (action: () => Promise<void> | void) => {
    try {
      await action();
    } catch (error: unknown) {
      props.onNotify(error instanceof Error ? error.message : 'Audio action failed', 'error');
    }
  };

  const client = new WebRtcIntercomClient({
    onConnectedChange: (value) => {
      setConnected(value);
      setConnecting(false);
      if (value && pendingTalk()) {
        void runAction(() => client.startTalk()).finally(() => setPendingTalk(false));
      } else if (!value) {
        setPendingTalk(false);
      }
    },
    onTalkingChange: setTalking,
    onTxLevelChange: setTxRms,
    onError: (msg) => props.onNotify(msg, 'error'),
  });

  onCleanup(() => client.disconnect());

  const rxHalf = () => props.rxGainHalfDb ?? 40;
  const txHalf = () => props.txGainHalfDb ?? 0;
  const rms = () => props.rxRmsDbfs ?? -96;
  const peak = () => props.rxPeakDbfs ?? -96;
  const meterPct = (dbfs: number) => Math.max(0, Math.min(100, ((dbfs + 96) / 96) * 100));

  type AudioMode = 'idle' | 'listen' | 'talk';
  const effectiveBus = () => props.busState ?? 'idle';
  const effectiveCall = () => props.callState ?? 'idle';

  const mode = (): AudioMode => {
    if (transport() === 'relay') {
      if (effectiveCall() === 'talking') return 'talk';
      if (effectiveCall() === 'listening' || effectiveCall() === 'settling') return 'listen';
      return 'idle';
    }
    if (talking() || pendingTalk()) return 'talk';
    if (connected() || connecting()) return 'listen';
    return 'idle';
  };

  const switchTransport = async (next: 'webrtc' | 'relay') => {
    if (next === transport()) return;
    setPendingTalk(false);
    setConnecting(false);
    client.setRemoteMuted(false);
    client.disconnect();
    if (effectiveCall() !== 'idle') {
      await runAction(() => props.onHangup());
    }
    setTransport(next);
  };

  const selectMode = async (next: AudioMode) => {
    if (transport() === 'relay') {
      if (next === 'idle') {
        await runAction(() => props.onHangup());
      } else if (next === 'listen') {
        if (effectiveCall() === 'talking') {
          await runAction(() => props.onStopTalk());
        } else {
          await runAction(() => props.onStartListen());
        }
      } else {
        await runAction(() => props.onStartTalk());
      }
      return;
    }

    if (next === 'idle') {
      setPendingTalk(false);
      setConnecting(false);
      client.setRemoteMuted(false);
      client.disconnect();
      return;
    }

    if (next === 'listen') {
      setPendingTalk(false);
      client.setRemoteMuted(false);
      if (talking()) {
        client.stopTalk();
        return;
      }
      if (!connected() && !connecting()) {
        setConnecting(true);
        try {
          await client.connect();
        } catch (e: unknown) {
          const msg = e instanceof Error ? e.message : 'Audio connection failed';
          setConnecting(false);
          props.onNotify(msg, 'error');
        }
      }
      return;
    }

    client.setRemoteMuted(true);
    if (connected()) {
      setPendingTalk(true);
      await runAction(() => client.startTalk()).finally(() => setPendingTalk(false));
      return;
    }

    setPendingTalk(true);
    if (!connecting()) {
      setConnecting(true);
      try {
        await client.connect();
      } catch (e: unknown) {
        const msg = e instanceof Error ? e.message : 'Audio connection failed';
        setPendingTalk(false);
        setConnecting(false);
        client.setRemoteMuted(false);
        props.onNotify(msg, 'error');
      }
    }
  };

  return (
    <Card>
      <CardBody class="gap-4">
        <div class="flex flex-wrap items-center justify-between gap-2">
          <CardTitle class="flex items-center gap-2 text-base">
            <Volume2 class="w-4 h-4 text-primary" />
            <span>Live Audio &amp; Bus Control</span>
          </CardTitle>
          <div class="flex flex-wrap items-center gap-2">
            <Badge variant={effectiveBus() === 'idle' ? 'neutral' : 'info'}>
              <span>relay: {effectiveBus()}</span>
            </Badge>
            <Badge variant={effectiveCall() === 'talking' ? 'warning' : 'neutral'}>
              <span>phase: {effectiveCall()}</span>
            </Badge>
            <fieldset class="join" aria-label="Control Mode">
              <button
                type="button"
                class={`btn btn-xs join-item ${
                  transport() === 'webrtc' ? 'btn-primary' : 'btn-outline'
                }`}
                onClick={() => void switchTransport('webrtc')}
              >
                WebRTC
              </button>
              <button
                type="button"
                class={`btn btn-xs join-item ${
                  transport() === 'relay' ? 'btn-secondary' : 'btn-outline'
                }`}
                onClick={() => void switchTransport('relay')}
              >
                Relay Only
              </button>
            </fieldset>
          </div>
        </div>

        <div class="flex flex-col gap-1">
          <div class="flex justify-between text-xs opacity-75">
            <span>RX Bus Level</span>
            <span>
              RMS {rms()} dBFS / Peak {peak()} dBFS
            </span>
          </div>
          <progress
            aria-label="RX Bus Level"
            class="progress progress-info w-full"
            value={meterPct(rms())}
            max="100"
          />
          <div class="flex justify-between text-xs opacity-75">
            <span>TX Microphone Level</span>
            <span>RMS {txRms()} dBFS</span>
          </div>
          <progress
            aria-label="TX Microphone Level"
            class="progress progress-warning w-full"
            value={meterPct(txRms())}
            max="100"
          />
        </div>

        <fieldset class="join w-full" aria-label="Audio Mode">
          <button
            type="button"
            class={`btn btn-sm join-item flex-1 gap-1.5 ${
              mode() === 'idle' ? 'btn-neutral' : 'btn-outline'
            }`}
            onClick={() => void selectMode('idle')}
          >
            <PhoneOff class="w-4 h-4" />
            <span>Idle</span>
          </button>
          <button
            type="button"
            class={`btn btn-sm join-item flex-1 gap-1.5 ${
              mode() === 'listen' ? 'btn-primary' : 'btn-outline'
            }`}
            onClick={() => void selectMode('listen')}
          >
            <Volume2 class="w-4 h-4" />
            <span>{connecting() && !pendingTalk() ? 'Connecting…' : 'Listen'}</span>
          </button>
          <button
            type="button"
            class={`btn btn-sm join-item flex-1 gap-1.5 ${
              talking() ? 'btn-error' : pendingTalk() ? 'btn-warning btn-outline' : 'btn-outline'
            }`}
            onClick={() => void selectMode('talk')}
          >
            {talking() ? (
              <span class="w-2 h-2 rounded-full bg-current animate-pulse" />
            ) : (
              <Mic class="w-4 h-4" />
            )}
            <span>{talking() ? 'Live (Speak)' : pendingTalk() ? 'Starting…' : 'Talk'}</span>
          </button>
        </fieldset>

        <div class="grid grid-cols-1 sm:grid-cols-2 gap-4 pt-1">
          <label class="form-control">
            <div class="label py-1 text-xs">
              <span>RX MicPGA Gain</span>
              <span>+{(rxHalf() / 2).toFixed(1)} dB</span>
            </div>
            <input
              type="range"
              aria-label="RX MicPGA Gain"
              step="1"
              min="0"
              max="95"
              value={rxHalf()}
              class="range range-xs range-info"
              onChange={(e) => {
                const value = Number(e.currentTarget.value);
                void runAction(() => props.onGainChange(value, txHalf()));
              }}
            />
          </label>

          <label class="form-control">
            <div class="label py-1 text-xs">
              <span>TX DAC Volume</span>
              <span>{(txHalf() / 2).toFixed(1)} dB</span>
            </div>
            <input
              type="range"
              aria-label="TX DAC Volume"
              step="1"
              min="-60"
              max="24"
              value={txHalf()}
              class="range range-xs range-warning"
              onChange={(e) => {
                const value = Number(e.currentTarget.value);
                void runAction(() => props.onGainChange(rxHalf(), value));
              }}
            />
          </label>
        </div>
      </CardBody>
    </Card>
  );
};
