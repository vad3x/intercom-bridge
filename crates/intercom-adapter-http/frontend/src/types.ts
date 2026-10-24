import type { SystemDiagnosticsBase } from '@embedded-device-foundation/dashboard-core';

export type {
  ActiveEcosystem,
  EndpointInfo,
  FabricInfo,
} from '@embedded-device-foundation/dashboard-core';

export interface SystemDiagnostics extends SystemDiagnosticsBase {
  door_locked: boolean;
}
