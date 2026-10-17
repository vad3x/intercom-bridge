import { Bell, Loader2, Lock, Sparkles, Unlock } from 'lucide-solid';
import type { Component } from 'solid-js';
import { Badge } from './ui/Badge';
import { Card, CardBody, CardTitle } from './ui/Card';

export interface IntercomControlsProps {
  doorLocked: boolean | undefined;
  isUnlocking: boolean;
  isRinging: boolean;
  onUnlock: () => void;
  onRing: () => void;
}

export const IntercomControls: Component<IntercomControlsProps> = (props) => {
  const isLocked = () => !props.isUnlocking && (props.doorLocked ?? true);

  return (
    <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
      {/* Door Strike / Lock Control Card */}
      <Card bordered>
        <CardBody class="flex flex-col justify-between">
          <div>
            <div class="flex items-center justify-between mb-4">
              <CardTitle>Door Access</CardTitle>
              <Badge variant={isLocked() ? 'success' : 'warning'}>
                {props.isUnlocking ? 'Unlocking...' : isLocked() ? 'Locked' : 'Unlocked'}
              </Badge>
            </div>

            <div class="flex flex-col items-center justify-center py-6 text-center">
              <div
                class={`w-24 h-24 rounded-full flex items-center justify-center transition-all duration-300 ${
                  isLocked()
                    ? 'bg-success/15 text-success'
                    : 'bg-warning/20 text-warning animate-pulse'
                }`}
              >
                {props.isUnlocking ? (
                  <Loader2 class="w-12 h-12 animate-spin" />
                ) : isLocked() ? (
                  <Lock class="w-12 h-12" />
                ) : (
                  <Unlock class="w-12 h-12" />
                )}
              </div>
              <p class="mt-4 font-semibold text-lg">
                {props.isUnlocking
                  ? 'Actuating Door Strike...'
                  : isLocked()
                    ? 'Entrance Door Secured'
                    : 'Entrance Door Unlocked'}
              </p>
              <p class="text-xs text-base-content/60 mt-1">
                PhotoMOS Relay • Auto-relocks after strike pulse
              </p>
            </div>
          </div>

          <button
            type="button"
            class="btn btn-primary w-full gap-2 text-base font-bold shadow-md"
            disabled={props.isUnlocking}
            onClick={props.onUnlock}
          >
            {props.isUnlocking ? (
              <>
                <Loader2 class="w-5 h-5 animate-spin" />
                Unlocking...
              </>
            ) : (
              <>
                <Unlock class="w-5 h-5" />
                Unlock Door
              </>
            )}
          </button>
        </CardBody>
      </Card>

      {/* Doorbell Ring Status & Test Card */}
      <Card bordered>
        <CardBody class="flex flex-col justify-between">
          <div>
            <div class="flex items-center justify-between mb-4">
              <CardTitle>Doorbell Sensor</CardTitle>
              <Badge variant={props.isRinging ? 'warning' : 'neutral'}>
                {props.isRinging ? 'Ringing' : 'Idle'}
              </Badge>
            </div>

            <div class="flex flex-col items-center justify-center py-6 text-center">
              <div
                class={`w-24 h-24 rounded-full flex items-center justify-center transition-all duration-300 ${
                  props.isRinging
                    ? 'bg-warning/20 text-warning animate-bounce'
                    : 'bg-base-200 text-base-content/60'
                }`}
              >
                <Bell class="w-12 h-12" />
              </div>
              <p class="mt-4 font-semibold text-lg">
                {props.isRinging ? 'Doorbell Ring Detected!' : 'Doorbell Chime Idle'}
              </p>
              <p class="text-xs text-base-content/60 mt-1">
                Optocoupler Input • Bridges analog chime to Matter Occupancy
              </p>
            </div>
          </div>

          <button
            type="button"
            class="btn btn-secondary w-full gap-2 text-base font-bold shadow-md"
            disabled={props.isRinging}
            onClick={props.onRing}
          >
            <Sparkles class="w-5 h-5" />
            Test Doorbell Ring
          </button>
        </CardBody>
      </Card>
    </div>
  );
};
