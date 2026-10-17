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
                  <td class="text-base-content/70">🔔 Ring / Occupancy (EP2)</td>
                  <td class="text-right">{renderSubBadge(propsObj().ring_sensor_subscribed)}</td>
                </tr>
              </tbody>
            </table>
          </div>

          <p class="text-xs text-base-content/60 mt-2">
            Readiness barrier ensures Apple HomeKit controllers prime cluster subscriptions before
            marking the bridge Operational.
          </p>
        </CardBody>
      </Card>

      {/* Smart Home Integration Info */}
      <Card bordered>
        <CardBody>
          <CardTitle class="text-base font-bold text-base-content mb-2">
            Ecosystem Integrations
          </CardTitle>
          <p class="text-sm text-base-content/80 mb-3">
            Intercom Bridge bridges analog intercom hardware to major smart home ecosystems via
            Matter.
          </p>
          <ul class="space-y-2 text-sm text-base-content/70">
            <li class="flex items-center gap-2">
              <Check class="w-4 h-4 text-success" />
              <span>Apple Home (HomeKit door lock + doorbell chime)</span>
            </li>
            <li class="flex items-center gap-2">
              <Check class="w-4 h-4 text-success" />
              <span>Home Assistant Matter integration</span>
            </li>
            <li class="flex items-center gap-2">
              <Check class="w-4 h-4 text-success" />
              <span>Google Home & Amazon Alexa compatible</span>
            </li>
          </ul>
        </CardBody>
      </Card>
    </div>
  );
};
