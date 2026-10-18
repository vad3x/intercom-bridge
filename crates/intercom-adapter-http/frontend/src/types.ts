export interface FabricInfo {
  fab_idx: number;
  vendor_id: number;
  fabric_id: string;
  node_id: string;
  label: string;
}

export interface EndpointInfo {
  id: number;
  name: string;
  cluster: string;
  ready: boolean;
}

export interface ActiveEcosystem {
  type: string;
  paired: boolean;
  properties: {
    door_lock_subscribed?: boolean;
    ring_sensor_subscribed?: boolean;
    [key: string]: boolean | number | string | undefined;
  };
  fabrics?: FabricInfo[];
  endpoints?: EndpointInfo[];
}

export interface SystemDiagnostics {
  state: string; // "Booting", "Starting", "Commissioning", "WifiConnecting", "Operational", "Updating", "Error"
  door_locked: boolean;
  uptime_sec: number;
  wifi_ssid: string | null;
  wifi_rssi_dbm: number | null;
  ip: string | null;
  mac: string;
  active_slot: string; // "ota_0" | "ota_1"
  slot_valid: boolean;
  free_heap_bytes: number;
  total_heap_bytes: number;
  available_ecosystems: string[];
  active_ecosystem: ActiveEcosystem | null;
  version: string;
  git_commit: string;
  build_date: string;
}
