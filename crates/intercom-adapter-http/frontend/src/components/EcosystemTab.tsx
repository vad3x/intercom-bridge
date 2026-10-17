import { Check, Cpu, Globe, Home, Layers, Radio, RotateCcw, TriangleAlert } from 'lucide-solid';
import { type Component, For, Show, createSignal } from 'solid-js';
import type { ActiveEcosystem, FabricInfo } from '../types';
import { Badge } from './ui/Badge';
import { Card, CardBody, CardTitle } from './ui/Card';

export interface EcosystemTabProps {
  ecosystem: ActiveEcosystem | null | undefined;
  onResetMatter?: () => Promise<void> | void;
  isResettingMatter?: boolean;
}

interface VendorMeta {
  name: string;
  role: string;
  iconType: 'apple' | 'home' | 'google' | 'alexa' | 'smartthings' | 'generic';
}

function resolveVendor(vid: number): VendorMeta {
  switch (vid) {
    case 0x1384: // 4996
      return { name: 'Apple Home', role: 'Resident Hub', iconType: 'apple' };
    case 0x1349: // 4937
      return { name: 'Apple Home', role: 'Commissioner', iconType: 'apple' };
    case 0x130d: // 4877
      return { name: 'Home Assistant', role: 'Matter Server', iconType: 'home' };
    case 0x18b4: // 6324
      return { name: 'Google Home', role: 'Matter Controller', iconType: 'google' };
    case 0x111d: // 4381
      return { name: 'Amazon Alexa', role: 'Echo / Hub', iconType: 'alexa' };
    case 0x1160: // 4448
      return { name: 'SmartThings', role: 'SmartThings Hub', iconType: 'smartthings' };
    case 0x115f: // 4447 - Lumi United Technology (Aqara)
      return { name: 'Aqara', role: 'Aqara Hub', iconType: 'smartthings' };
    case 0xfff1:
    case 0xfff2:
    case 0xfff3:
    case 0xfff4:
      return { name: 'Test / Development', role: 'Matter Controller', iconType: 'generic' };
    default:
      return {
        name: `Vendor 0x${vid.toString(16).toUpperCase()}`,
        role: 'Matter Controller',
        iconType: 'generic',
      };
  }
}

const renderVendorIcon = (iconType: VendorMeta['iconType']) => {
  switch (iconType) {
    case 'apple':
      return (
        <svg
          class="w-4 h-4 fill-current text-base-content"
          viewBox="0 0 170 170"
          role="img"
          aria-label="Apple logo"
        >
          <title>Apple logo</title>
          <path d="M150.37 130.25c-2.45 5.66-5.35 10.87-8.71 15.66-4.58 6.53-8.33 11.05-11.22 13.56-4.48 4.12-9.28 6.23-14.42 6.35-3.69 0-8.14-1.05-13.32-3.18-5.19-2.12-9.97-3.17-14.34-3.17-4.58 0-9.49 1.05-14.75 3.17-5.26 2.13-9.5 3.24-12.74 3.35-4.35.13-9.16-1.9-14.42-6.08-3.69-3.08-7.7-7.94-12.04-14.59-6.3-9.69-11.22-20.9-14.75-33.64-3.52-12.74-5.28-24.84-5.28-36.3 0-14.82 3.65-27.24 10.96-37.28 7.3-10.03 16.79-15.16 28.45-15.38 5.34 0 11.23 1.48 17.67 4.45 6.44 2.97 10.4 4.51 11.89 4.63 1.95-.33 6.35-2.03 13.2-5.1 6.84-3.07 12.87-4.43 18.09-4.08 13.9.76 25.12 5.86 33.68 15.31-12.18 7.39-18.17 17.5-17.96 30.34.22 10.02 4.12 18.52 11.71 25.49 7.59 6.97 16.48 10.89 26.68 11.75-2.39 7.18-5.11 14.18-8.17 21-.01.03-3.23 7.42-3.23 7.42zM119.22 33.09c0-7.39 2.65-14.47 7.94-21.24 5.3-6.77 12.01-11.16 20.14-13.16 1.08 7.61-.83 15.11-5.74 22.5-4.91 7.39-11.38 11.7-19.41 12.93-.87-.33-1.89-.53-2.93-.53v-.5z" />
        </svg>
      );
    case 'home':
      return <Home class="w-4 h-4 text-sky-400" />;
    case 'google':
      return <Globe class="w-4 h-4 text-emerald-400" />;
    case 'alexa':
      return <Radio class="w-4 h-4 text-cyan-400" />;
    case 'smartthings':
      return <Cpu class="w-4 h-4 text-blue-400" />;
    default:
      return <Layers class="w-4 h-4 text-base-content/70" />;
  }
};

const FabricRow: Component<{ fabric: FabricInfo }> = (props) => {
  const [expanded, setExpanded] = createSignal(false);
  const meta = () => resolveVendor(props.fabric.vendor_id);

  return (
    <div class="bg-base-200/50 rounded-lg border border-base-content/10 p-3 hover:border-primary/40 transition-colors">
      <div class="flex items-center justify-between gap-2">
        <div class="flex items-center gap-3 min-w-0">
          <div class="w-8 h-8 rounded-lg bg-base-300 flex items-center justify-center shrink-0">
            {renderVendorIcon(meta().iconType)}
          </div>
          <div class="min-w-0">
            <div class="flex items-center gap-2 flex-wrap">
              <span class="font-semibold text-sm text-base-content truncate">{meta().name}</span>
              <span class="text-[11px] px-1.5 py-0.5 rounded bg-base-300 text-base-content/70 font-medium">
                {meta().role}
              </span>
            </div>
            <div class="text-xs text-base-content/60 truncate">
              {props.fabric.label ? `${props.fabric.label} • ` : ''}Fabric #{props.fabric.fab_idx}
            </div>
          </div>
        </div>
        <div class="flex items-center gap-1.5 shrink-0">
          <Badge variant="success" class="text-xs">
            Connected
          </Badge>
          <button
            type="button"
            onClick={() => setExpanded(!expanded())}
            class="btn btn-ghost btn-xs text-primary font-medium"
          >
            {expanded() ? 'Less ▲' : 'Details ▼'}
          </button>
        </div>
      </div>

      <Show when={expanded()}>
        <div class="mt-3 pt-2 border-t border-base-content/10 text-xs grid grid-cols-1 sm:grid-cols-2 gap-1.5 text-base-content/70 font-mono">
          <div>
            <span class="text-base-content/50">VID:</span> 0x
            {props.fabric.vendor_id.toString(16).toUpperCase()}
          </div>
          <div>
            <span class="text-base-content/50">Fabric Index:</span> #{props.fabric.fab_idx}
          </div>
          <div class="truncate">
            <span class="text-base-content/50">Node ID:</span> {props.fabric.node_id}
          </div>
          <div class="truncate">
            <span class="text-base-content/50">Fabric ID:</span> {props.fabric.fabric_id}
          </div>
          <Show when={props.fabric.label}>
            <div class="sm:col-span-2 truncate">
              <span class="text-base-content/50">Label:</span> {props.fabric.label}
            </div>
          </Show>
        </div>
      </Show>
    </div>
  );
};

export const EcosystemTab: Component<EcosystemTabProps> = (props) => {
  const [confirmReset, setConfirmReset] = createSignal(false);
  const isPaired = () => props.ecosystem?.paired ?? false;
  const propsObj = () => props.ecosystem?.properties || {};
  const fabrics = () => props.ecosystem?.fabrics || [];

  const renderSubBadge = (val: boolean | undefined) => {
    if (val === true) {
      return <Badge variant="success">Subscribed</Badge>;
    }
    if (val === false) {
      return <Badge variant="warning">Waiting</Badge>;
    }
    return <Badge variant="neutral">Ready</Badge>;
  };

  return (
    <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
      {/* Matter Ecosystem Card */}
      <Card bordered>
        <CardBody class="space-y-4">
          {/* Header */}
          <div class="flex items-center justify-between mb-1">
            <CardTitle class="text-base font-bold text-base-content m-0">
              Matter over Wi-Fi
            </CardTitle>
            <div class="flex items-center gap-2">
              <Show when={fabrics().length > 0}>
                <Badge variant="info" class="font-medium text-xs">
                  {fabrics().length} {fabrics().length === 1 ? 'Hub' : 'Hubs'}
                </Badge>
              </Show>
              <Badge variant={isPaired() ? 'success' : 'warning'}>
                {isPaired() ? 'Active' : 'Unpaired'}
              </Badge>
            </div>
          </div>

          {/* Endpoints Subscriptions */}
          <div class="overflow-x-auto my-1">
            <table class="table table-sm">
              <tbody>
                <tr class="border-b border-base-content/10">
                  <td class="text-base-content/70">Fabric Commissioning</td>
                  <td class="text-right">
                    <Badge variant={isPaired() ? 'success' : 'warning'}>
                      {isPaired() ? 'Paired' : 'Unpaired'}
                    </Badge>
                  </td>
                </tr>
                <tr class="border-b border-base-content/10">
                  <td class="text-base-content/70">🔒 Door Lock (EP1)</td>
                  <td class="text-right">{renderSubBadge(propsObj().door_lock_subscribed)}</td>
                </tr>
                <tr class="border-b border-base-content/10">
                  <td class="text-base-content/70">🔔 Chime / Ring (EP2)</td>
                  <td class="text-right">{renderSubBadge(propsObj().ring_sensor_subscribed)}</td>
                </tr>
              </tbody>
            </table>
          </div>

          {/* Connected Fabrics & Hubs Section */}
          <div class="pt-2">
            <div class="flex items-center justify-between mb-2">
              <span class="text-xs font-bold uppercase tracking-wider text-base-content/60">
                Connected Hubs & Fabrics
              </span>
              <span class="text-xs text-base-content/50">{fabrics().length} of 5 fabrics used</span>
            </div>

            <Show
              when={fabrics().length > 0}
              fallback={
                <div class="bg-base-200/40 rounded-lg p-3 text-center text-xs text-base-content/60 border border-base-content/10">
                  Waiting for commissioning (BLE / Wi-Fi)
                </div>
              }
            >
              <div class="space-y-2">
                <For each={fabrics()}>{(fab) => <FabricRow fabric={fab} />}</For>
              </div>
            </Show>
          </div>

          <div class="mt-4 pt-2">
            <button
              type="button"
              class="btn btn-sm btn-block btn-success btn-outline pointer-events-none"
            >
              <Check class="w-4 h-4 mr-1" /> Active Ecosystem
            </button>
          </div>

          {/* Matter KVS Reset Action */}
          <div class="mt-2 pt-2 border-t border-base-content/10">
            <Show
              when={confirmReset()}
              fallback={
                <button
                  type="button"
                  onClick={() => setConfirmReset(true)}
                  disabled={props.isResettingMatter}
                  class="btn btn-sm btn-outline btn-error btn-block"
                >
                  <RotateCcw class="w-4 h-4 mr-1" />
                  Reset Matter KVS
                </button>
              }
            >
              <div class="bg-error/10 border border-error/30 rounded-lg p-3 space-y-2">
                <div class="text-xs text-error font-semibold flex items-center gap-1.5">
                  <TriangleAlert class="w-4 h-4 shrink-0" />
                  Reset Matter KVS Storage?
                </div>
                <p class="text-[11px] text-base-content/70 leading-normal">
                  This will unpair all connected hubs, erase all Matter fabric credentials, and
                  reboot the device. Wi-Fi credentials will remain intact.
                </p>
                <div class="flex gap-2 pt-1">
                  <button
                    type="button"
                    onClick={async () => {
                      setConfirmReset(false);
                      await props.onResetMatter?.();
                    }}
                    disabled={props.isResettingMatter}
                    class="btn btn-xs btn-error flex-1"
                  >
                    <Show when={props.isResettingMatter} fallback="Confirm Reset">
                      <span class="loading loading-spinner loading-xs mr-1" />
                      Resetting...
                    </Show>
                  </button>
                  <button
                    type="button"
                    onClick={() => setConfirmReset(false)}
                    disabled={props.isResettingMatter}
                    class="btn btn-xs btn-ghost"
                  >
                    Cancel
                  </button>
                </div>
              </div>
            </Show>
          </div>
        </CardBody>
      </Card>

      {/* Home Assistant Planned Card */}
      <Card bordered class="opacity-80">
        <CardBody>
          <div class="flex items-center justify-between mb-2">
            <CardTitle class="text-base font-bold text-base-content m-0">Home Assistant</CardTitle>
            <Badge variant="ghost">Planned</Badge>
          </div>

          <p class="text-sm text-base-content/70 my-4 leading-relaxed">
            Native direct integration for Home Assistant via ESPHome/native API. Currently in
            planning.
          </p>

          <div class="mt-auto pt-4">
            <button type="button" class="btn btn-sm btn-block btn-disabled">
              Switch Ecosystem (Coming Soon)
            </button>
          </div>
        </CardBody>
      </Card>
    </div>
  );
};
