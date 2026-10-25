import { Badge, Card, CardBody, CardTitle } from '@embedded-device-foundation/dashboard-core';
import { Bell, Loader2, Lock, Sparkles, Unlock } from 'lucide-solid';
import { type Component, createEffect, createSignal } from 'solid-js';
import {
  DEFAULT_DOOR_UNLOCK_DURATION_SEC,
  MAX_DOOR_UNLOCK_DURATION_SEC,
  MIN_DOOR_UNLOCK_DURATION_SEC,
} from '../types';

export interface IntercomControlsProps {
  doorLocked: boolean | undefined;
  doorUnlockDurationSec?: number;
  isUnlocking: boolean;
  isRinging: boolean;
  onUnlock: () => void;
  onRing: () => void;
  onDurationChange?: (seconds: number) => Promise<void> | void;
}

export const IntercomControls: Component<IntercomControlsProps> = (props) => {
  const isInitialLoading = () => props.doorLocked === undefined;
  const isLocked = () => !props.isUnlocking && props.doorLocked === true;

  const [draftDuration, setDraftDuration] = createSignal<number | undefined>(
    props.doorUnlockDurationSec,
  );
  const [userEdited, setUserEdited] = createSignal(false);
  const [isSaving, setIsSaving] = createSignal(false);

  // Sync draft from server whenever server updates and user hasn't actively edited
  createEffect(() => {
    const serverVal = props.doorUnlockDurationSec;
    if (serverVal !== undefined && !userEdited() && !isSaving()) {
      setDraftDuration(serverVal);
    }
  });

  const effectiveDuration = () =>
    draftDuration() ?? props.doorUnlockDurationSec ?? DEFAULT_DOOR_UNLOCK_DURATION_SEC;

  const isDirty = () => {
    if (props.doorUnlockDurationSec === undefined) return false;
    return userEdited() && draftDuration() !== props.doorUnlockDurationSec;
  };

  const handleStep = (delta: number) => {
    const current = effectiveDuration();
    const next = Math.max(
      MIN_DOOR_UNLOCK_DURATION_SEC,
      Math.min(MAX_DOOR_UNLOCK_DURATION_SEC, current + delta),
    );
    setDraftDuration(next);
    setUserEdited(
      props.doorUnlockDurationSec !== undefined && next !== props.doorUnlockDurationSec,
    );
  };

  const handleSave = async () => {
    if (!isDirty() || isSaving()) return;
    const val = effectiveDuration();
    setIsSaving(true);
    try {
      await props.onDurationChange?.(val);
      setUserEdited(false);
    } finally {
      setIsSaving(false);
    }
  };

  const strikeIconClass = () => {
    if (isInitialLoading()) return 'bg-base-300 text-base-content/40';
    return isLocked() ? 'bg-success/20 text-success' : 'bg-warning/20 text-warning';
  };

  const strikeSubtitle = () => {
    if (isInitialLoading()) return 'Checking status...';
    if (props.isUnlocking) return 'Actuating Strike';
    return isLocked() ? 'Secured (LOCKED)' : 'Unlatched (UNLOCKED)';
  };

  const strikeBadge = () => {
    if (isInitialLoading()) {
      return {
        variant: 'neutral' as const,
        dotClass: 'bg-base-content/30 animate-pulse',
        text: 'connecting',
      };
    }
    if (props.isUnlocking) {
      return {
        variant: 'warning' as const,
        dotClass: 'bg-warning animate-pulse',
        text: 'unlocking',
      };
    }
    if (isLocked()) {
      return {
        variant: 'success' as const,
        dotClass: 'bg-success',
        text: 'locked',
      };
    }
    return {
      variant: 'warning' as const,
      dotClass: 'bg-warning animate-pulse',
      text: 'unlocked',
    };
  };

  const strikeHeadline = () => {
    if (isInitialLoading()) {
      return { text: 'Connecting...', class: 'text-base-content/40' };
    }
    if (props.isUnlocking) {
      return { text: 'Unlocking...', class: 'text-warning' };
    }
    if (isLocked()) {
      return { text: 'Locked', class: 'text-success' };
    }
    return { text: 'Unlocked', class: 'text-warning' };
  };

  return (
    <div class="space-y-4">
      <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
        {/* Door Access Tile */}
        <Card bordered>
          <CardBody>
            <div class="flex items-center justify-between pb-3 border-b border-base-content/10">
              <div class="flex items-center gap-2.5">
                <div class={`p-2 rounded-lg transition-colors ${strikeIconClass()}`}>
                  {isLocked() || isInitialLoading() ? (
                    <Lock class="w-4 h-4" />
                  ) : (
                    <Unlock class="w-4 h-4" />
                  )}
                </div>
                <div>
                  <div class="font-semibold text-sm text-base-content">Door Strike</div>
                  <div class="text-xs text-base-content/60">{strikeSubtitle()}</div>
                </div>
              </div>
              <Badge variant={strikeBadge().variant}>
                <span class={`w-2 h-2 rounded-full ${strikeBadge().dotClass}`} />
                <span>{strikeBadge().text}</span>
              </Badge>
            </div>

            <div class="flex flex-col items-center justify-center py-4 text-center">
              <CardTitle class="mb-2">Entrance Door</CardTitle>
              <div class={`text-3xl sm:text-4xl font-black my-1 ${strikeHeadline().class}`}>
                {strikeHeadline().text}
              </div>
              <div class="text-xs text-base-content/60 mt-1">
                PhotoMOS Relay • Auto-relocks after {effectiveDuration()}s strike pulse
              </div>
              <div class="flex items-center justify-center gap-2 mt-3">
                <span class="text-xs text-base-content/70">Duration:</span>
                <div class="join border border-base-content/20 rounded-lg">
                  <button
                    type="button"
                    class="join-item btn btn-xs btn-ghost px-2 font-bold"
                    disabled={
                      props.isUnlocking ||
                      isSaving() ||
                      effectiveDuration() <= MIN_DOOR_UNLOCK_DURATION_SEC
                    }
                    onClick={() => handleStep(-1)}
                  >
                    -
                  </button>
                  <span class="join-item px-2.5 py-0.5 text-xs font-mono font-bold flex items-center bg-base-200">
                    {effectiveDuration()}s
                  </span>
                  <button
                    type="button"
                    class="join-item btn btn-xs btn-ghost px-2 font-bold"
                    disabled={
                      props.isUnlocking ||
                      isSaving() ||
                      effectiveDuration() >= MAX_DOOR_UNLOCK_DURATION_SEC
                    }
                    onClick={() => handleStep(1)}
                  >
                    +
                  </button>
                </div>
                <button
                  type="button"
                  class="btn btn-xs btn-primary gap-1"
                  disabled={!isDirty() || isSaving() || props.isUnlocking}
                  onClick={handleSave}
                >
                  {isSaving() ? (
                    <>
                      <Loader2 class="w-3 h-3 animate-spin" />
                      <span>Saving...</span>
                    </>
                  ) : (
                    'Save'
                  )}
                </button>
              </div>
              <div class="w-full mt-5">
                <button
                  type="button"
                  class="btn btn-primary w-full gap-2 font-medium"
                  disabled={props.isUnlocking}
                  onClick={props.onUnlock}
                >
                  {props.isUnlocking ? (
                    <>
                      <Loader2 class="w-4 h-4 animate-spin" />
                      Unlocking Door...
                    </>
                  ) : (
                    <>
                      <Unlock class="w-4 h-4" />
                      Unlock Entrance Door
                    </>
                  )}
                </button>
              </div>
            </div>
          </CardBody>
        </Card>

        {/* Doorbell Chime Sensor Tile */}
        <Card bordered>
          <CardBody>
            <div class="flex items-center justify-between pb-3 border-b border-base-content/10">
              <div class="flex items-center gap-2.5">
                <div
                  class={`p-2 rounded-lg transition-colors ${
                    props.isRinging
                      ? 'bg-warning/20 text-warning'
                      : 'bg-base-300 text-base-content/60'
                  }`}
                >
                  <Bell class="w-4 h-4" />
                </div>
                <div>
                  <div class="font-semibold text-sm text-base-content">Chime Sensor</div>
                  <div class="text-xs text-base-content/60">
                    {props.isRinging ? 'Ring Detected' : 'Monitoring Input'}
                  </div>
                </div>
              </div>
              <Badge variant={props.isRinging ? 'warning' : 'ghost'}>
                <span
                  class={`w-2 h-2 rounded-full ${
                    props.isRinging ? 'bg-warning animate-pulse' : 'bg-base-content/30'
                  }`}
                />
                <span>{props.isRinging ? 'ringing' : 'idle'}</span>
              </Badge>
            </div>

            <div class="flex flex-col items-center justify-center py-4 text-center">
              <CardTitle class="mb-2">Doorbell Chime</CardTitle>
              <div
                class={`text-3xl sm:text-4xl font-black my-1 ${
                  props.isRinging ? 'text-warning' : 'text-base-content/80'
                }`}
              >
                {props.isRinging ? 'Ringing!' : 'Idle'}
              </div>
              <div class="text-xs text-base-content/60 mt-1">
                Optocoupler Input • Matter Occupancy Endpoint 2
              </div>
              <div class="w-full mt-5">
                <button
                  type="button"
                  class="btn btn-outline btn-secondary w-full gap-2 font-medium"
                  disabled={props.isRinging}
                  onClick={props.onRing}
                >
                  <Sparkles class="w-4 h-4" />
                  Test Doorbell Ring
                </button>
              </div>
            </div>
          </CardBody>
        </Card>
      </div>
    </div>
  );
};
