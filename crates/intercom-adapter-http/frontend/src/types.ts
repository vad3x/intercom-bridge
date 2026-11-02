import type { SystemDiagnosticsBase } from '@embedded-device-foundation/dashboard-core';

export type {
  ActiveEcosystem,
  EndpointInfo,
  FabricInfo,
} from '@embedded-device-foundation/dashboard-core';

export const MIN_DOOR_UNLOCK_DURATION_SEC = 1;
export const MAX_DOOR_UNLOCK_DURATION_SEC = 30;
export const DEFAULT_DOOR_UNLOCK_DURATION_SEC = 5;

export type BusLineState = 'idle' | 'listen' | 'talk' | 'door_unlock';
export type CallState = 'idle' | 'listening' | 'talking' | 'settling' | 'unlocking';

export interface IntercomStatus {
  door_locked: boolean;
  door_unlock_duration_sec?: number;
  bus_state?: BusLineState;
  call_state?: CallState;
  rx_rms_dbfs?: number;
  rx_peak_dbfs?: number;
  rx_gain_half_db?: number;
  tx_gain_half_db?: number;
  tone_active?: boolean;
}

export interface SystemDiagnostics extends SystemDiagnosticsBase, IntercomStatus {}
