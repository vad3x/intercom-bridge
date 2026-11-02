import { Badge, Card, CardBody, CardTitle } from '@embedded-device-foundation/dashboard-core';
import { Headphones, Mic, PhoneOff, Radio } from 'lucide-solid';
import type { Component } from 'solid-js';
import type { BusLineState, CallState } from '../types';
import { createPttHandlers } from './pttHandlers';

export interface BusDebugCardProps {
  busState: BusLineState | undefined;
  callState: CallState | undefined;
  onStartListen: () => Promise<void> | void;
  onStartTalk: () => Promise<void> | void;
  onStopTalk: () => Promise<void> | void;
  onHangup: () => Promise<void> | void;
}

export const BusDebugCard: Component<BusDebugCardProps> = (props) => {
  const effectiveBus = () => props.busState ?? 'idle';
  const effectiveCall = () => props.callState ?? 'idle';
  const isSessionActive = () =>
    effectiveCall() === 'listening' ||
    effectiveCall() === 'talking' ||
    effectiveCall() === 'settling';

  const { handlePttDown, handlePttUp } = createPttHandlers(
    () => props.onStartTalk(),
    () => props.onStopTalk(),
  );

  return (
    <Card>
      <CardBody class="gap-4">
        <div class="flex items-center justify-between">
          <CardTitle class="flex items-center gap-2 text-base">
            <Radio class="w-4 h-4 text-primary" />
            <span>4-Wire Bus Relay Control (PCB v2)</span>
          </CardTitle>
          <div class="flex items-center gap-2">
            <Badge variant={effectiveBus() === 'idle' ? 'neutral' : 'info'}>
              <span>relay: {effectiveBus()}</span>
            </Badge>
            <Badge variant={effectiveCall() === 'talking' ? 'warning' : 'neutral'}>
              <span>phase: {effectiveCall()}</span>
            </Badge>
          </div>
        </div>

        <div class="grid grid-cols-1 sm:grid-cols-3 gap-2">
          <button
            type="button"
            class="btn btn-sm btn-outline btn-info gap-2"
            onClick={() => void props.onStartListen()}
          >
            <Headphones class="w-4 h-4" />
            <span>Listen (IO_LISTEN)</span>
          </button>

          <button
            type="button"
            class="btn btn-sm btn-warning gap-2 select-none touch-none"
            onPointerDown={handlePttDown}
            onPointerUp={handlePttUp}
            onPointerCancel={handlePttUp}
            onPointerLeave={handlePttUp}
          >
            <Mic class="w-4 h-4" />
            <span>Hold to Talk (IO_TALK)</span>
          </button>

          <button
            type="button"
            class="btn btn-sm btn-outline btn-error gap-2"
            disabled={!isSessionActive()}
            onClick={() => void props.onHangup()}
          >
            <PhoneOff class="w-4 h-4" />
            <span>Hang Up</span>
          </button>
        </div>
      </CardBody>
    </Card>
  );
};
