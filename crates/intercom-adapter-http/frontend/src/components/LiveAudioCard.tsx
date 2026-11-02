import { Badge, Card, CardBody, CardTitle } from '@embedded-device-foundation/dashboard-core';
import { AudioWaveform, Mic, PhoneCall, PhoneOff, Volume2 } from 'lucide-solid';
import { type Component, createEffect, createSignal, onCleanup } from 'solid-js';
import { WsIntercomClient } from '../audio/wsIntercomClient';
import { createPttHandlers } from './pttHandlers';
import { createToneCountdown } from './toneCountdown';

export interface LiveAudioCardProps {
  rxRmsDbfs: number | undefined;
  rxPeakDbfs: number | undefined;
  rxGainHalfDb: number | undefined;
  txGainHalfDb: number | undefined;
  toneActive: boolean | undefined;
  onGainChange: (rxHalfDb: number, txHalfDb: number) => Promise<void> | void;
  onToggleTone: (enable: boolean) => Promise<void> | void;
  onNotify: (msg: string, kind: 'info' | 'success' | 'error') => void;
}

export const LiveAudioCard: Component<LiveAudioCardProps> = (props) => {
  const [connected, setConnected] = createSignal(false);
  const [talking, setTalking] = createSignal(false);
  const [connecting, setConnecting] = createSignal(false);
  const [txRms, setTxRms] = createSignal(-96);
  const [toneSeconds, setToneSeconds] = createSignal<number | undefined>();
  const updateCountdown = createToneCountdown();

  createEffect(() => {
    const active = props.toneActive ?? false;
    setToneSeconds(updateCountdown(active, Date.now()));
    if (!active) return;
    const timer = setInterval(() => setToneSeconds(updateCountdown(true, Date.now())), 1000);
    onCleanup(() => clearInterval(timer));
  });

  const client = new WsIntercomClient({
    onConnectedChange: (value) => {
      setConnected(value);
      setConnecting(false);
    },
    onTalkingChange: setTalking,
    onTxLevelChange: setTxRms,
    onPreempted: () => props.onNotify('Live audio preempted by Matter call or OTA', 'info'),
    onError: (msg) => props.onNotify(msg, 'error'),
  });

  onCleanup(() => client.disconnect());

  const rxHalf = () => props.rxGainHalfDb ?? 40;
  const txHalf = () => props.txGainHalfDb ?? 0;
  const rms = () => props.rxRmsDbfs ?? -96;
  const peak = () => props.rxPeakDbfs ?? -96;
  const meterPct = (dbfs: number) => Math.max(0, Math.min(100, ((dbfs + 96) / 96) * 100));

  const toggleConnection = async () => {
    if (connected()) {
      client.disconnect();
    } else {
      setConnecting(true);
      try {
        await client.connect();
      } catch (e: unknown) {
        const msg = e instanceof Error ? e.message : 'Audio connection failed';
        setConnecting(false);
        props.onNotify(msg, 'error');
      }
    }
  };

  const runAction = async (action: () => Promise<void> | void) => {
    try {
      await action();
    } catch (error: unknown) {
      props.onNotify(error instanceof Error ? error.message : 'Audio action failed', 'error');
    }
  };

  const { handlePttDown, handlePttUp } = createPttHandlers(
    () => runAction(() => client.startTalk()),
    () => client.stopTalk(),
  );

  return (
    <Card>
      <CardBody class="gap-4">
        <div class="flex items-center justify-between">
          <CardTitle class="flex items-center gap-2 text-base">
            <Volume2 class="w-4 h-4 text-primary" />
            <span>Live Browser PTT &amp; Codec Calibration</span>
          </CardTitle>
          <Badge variant={connected() ? 'success' : 'neutral'}>
            <span>{connected() ? (talking() ? 'tx talking' : 'rx listening') : 'offline'}</span>
          </Badge>
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

        <div class="grid grid-cols-1 sm:grid-cols-3 gap-2">
          <button
            type="button"
            class={`btn btn-sm gap-2 ${connected() ? 'btn-error btn-outline' : 'btn-primary'}`}
            disabled={connecting()}
            onClick={() => void toggleConnection()}
          >
            {connected() ? <PhoneOff class="w-4 h-4" /> : <PhoneCall class="w-4 h-4" />}
            <span>
              {connecting()
                ? 'Connecting Audio…'
                : connected()
                  ? 'Disconnect Audio'
                  : 'Connect Live Audio'}
            </span>
          </button>

          <button
            type="button"
            class="btn btn-sm btn-warning gap-2 select-none touch-none"
            disabled={!connected()}
            onPointerDown={handlePttDown}
            onPointerUp={handlePttUp}
            onPointerCancel={handlePttUp}
            onPointerLeave={handlePttUp}
          >
            <Mic class="w-4 h-4" />
            <span>Hold to Speak (PTT)</span>
          </button>

          <button
            type="button"
            class={`btn btn-sm gap-2 ${props.toneActive ? 'btn-accent' : 'btn-outline'}`}
            onClick={() => void runAction(() => props.onToggleTone(!props.toneActive))}
          >
            <AudioWaveform class="w-4 h-4" />
            <span>{props.toneActive ? 'Stop 1 kHz Tone' : '1 kHz Test Tone'}</span>
            <span title="Estimated from when this dashboard observed the tone; the device enforces the 60s cutoff.">
              {toneSeconds() === undefined ? '' : `~${toneSeconds()}s left`}
            </span>
          </button>
        </div>

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
