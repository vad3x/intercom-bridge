import { Badge, Card, CardBody, CardTitle } from '@embedded-device-foundation/dashboard-core';
import type { Component } from 'solid-js';

export const IntercomEcosystemSupplementalPanel: Component = () => (
  <Card bordered class="opacity-80">
    <CardBody>
      <div class="flex items-center justify-between mb-2">
        <CardTitle class="text-base font-bold text-base-content m-0">Home Assistant</CardTitle>
        <Badge variant="ghost">Planned</Badge>
      </div>
      <p class="text-sm text-base-content/70 my-4 leading-relaxed">
        Native direct integration for Home Assistant via ESPHome/native API. Currently in planning.
      </p>
      <div class="mt-auto pt-4">
        <button type="button" class="btn btn-sm btn-block btn-disabled">
          Switch Ecosystem (Coming Soon)
        </button>
      </div>
    </CardBody>
  </Card>
);
