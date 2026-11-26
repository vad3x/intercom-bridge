import { createDashboardApi } from '@embedded-device-foundation/dashboard-core';
import type { IntercomDeviceState, IntercomDiagnostics } from '../src/types';

const deviceState: IntercomDeviceState = {
  door_locked: true,
  door_unlock_duration_sec: 5,
};

const validDiagnostics: IntercomDiagnostics = {
  system: {
    status: 'Operational',
    uptime_sec: 14250,
    memory: {
      internal: { free_bytes: 151552, total_bytes: 215040 },
      psram: null,
    },
    cpu: {},
  },
  network: {
    ssid: 'Home_IoT',
    rssi_dbm: -58,
    ip: '192.168.1.142',
    mac: '98:A3:16:A6:78:44',
  },
  firmware: {
    version: '1.0.0',
    git_commit: 'a1b2c3d',
    build_date: '2026-11-26',
    active_slot: 'ota_0',
    slot_valid: true,
  },
  ecosystems: {
    available: ['Matter'],
    active: {
      type: 'Matter',
      paired: true,
      properties: { door_lock_subscribed: true },
      fabrics: [],
      endpoints: [],
    },
  },
  device: {
    type: 'intercom_bridge',
    state: deviceState,
  },
};

const typedApi = createDashboardApi<IntercomDiagnostics>();
const lockedFlag: boolean = validDiagnostics.device.state.door_locked;

void typedApi;
void lockedFlag;

const legacyDoorLocked: IntercomDiagnostics = {
  ...validDiagnostics,
  // @ts-expect-error legacy flat door_locked key must not exist on IntercomDiagnostics
  door_locked: true,
};

const legacyDoorDuration: IntercomDiagnostics = {
  ...validDiagnostics,
  // @ts-expect-error legacy flat door_unlock_duration_sec key must not exist on IntercomDiagnostics
  door_unlock_duration_sec: 5,
};

const legacyState: IntercomDiagnostics = {
  ...validDiagnostics,
  // @ts-expect-error legacy flat state key must not exist on IntercomDiagnostics
  state: 'Operational',
};

const legacyActiveEcosystem: IntercomDiagnostics = {
  ...validDiagnostics,
  // @ts-expect-error legacy flat active_ecosystem key must not exist on IntercomDiagnostics
  active_ecosystem: null,
};

void legacyDoorLocked;
void legacyDoorDuration;
void legacyState;
void legacyActiveEcosystem;
