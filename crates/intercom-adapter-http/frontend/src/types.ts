import type { SystemDiagnosticsBase } from '@embedded-device-foundation/dashboard-core';

export type {
  ActiveEcosystem,
  EndpointInfo,
  FabricInfo,
} from '@embedded-device-foundation/dashboard-core';

export const MIN_DOOR_UNLOCK_DURATION_SEC = 1;
export const MAX_DOOR_UNLOCK_DURATION_SEC = 30;
export const DEFAULT_DOOR_UNLOCK_DURATION_SEC = 5;

export interface IntercomStatus {
  door_locked: boolean;
  door_unlock_duration_sec?: number;
}

export interface SystemDiagnostics extends SystemDiagnosticsBase, IntercomStatus {}
