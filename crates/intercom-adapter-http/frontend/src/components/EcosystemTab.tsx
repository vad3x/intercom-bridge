import { Check } from 'lucide-solid';
import type { Component } from 'solid-js';
import type { ActiveEcosystem } from '../types';
import { Badge } from './ui/Badge';
import { Card, CardBody, CardTitle } from './ui/Card';

export interface EcosystemTabProps {
  ecosystem: ActiveEcosystem | null | undefined;
}

export const EcosystemTab: Component<EcosystemTabProps> = (props) => {
  const isPaired = () => props.ecosystem?.paired ?? false;
  const propsObj = () => props.ecosystem?.properties || {};

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
        <CardBody>
          <div class="flex items-center justify-between mb-2">
            <CardTitle class="text-base font-bold text-base-content m-0">
              Matter over Wi-Fi
            </CardTitle>
            <Badge variant="success">Active</Badge>
          </div>

          <div class="overflow-x-auto my-2">
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

          <div class="mt-4">
            <button
              type="button"
              class="btn btn-sm btn-block btn-success btn-outline pointer-events-none"
            >
              <Check class="w-4 h-4 mr-1" /> Active Ecosystem
            </button>
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
