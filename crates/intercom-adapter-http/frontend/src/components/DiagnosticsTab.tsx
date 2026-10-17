import type { Component } from 'solid-js';
import type { SystemDiagnostics } from '../types';
import { Badge } from './ui/Badge';
import { Card, CardBody, CardTitle } from './ui/Card';

export interface DiagnosticsTabProps {
  status: SystemDiagnostics | null | undefined;
}

export const DiagnosticsTab: Component<DiagnosticsTabProps> = (props) => {
  const heapInfo = () => {
    const total = props.status?.total_heap_bytes ?? 0;
    const free = props.status?.free_heap_bytes ?? 0;
    if (total <= 0) return { usedKb: '0', totalKb: '0', pct: 0 };
    const used = Math.max(0, total - free);
    const pct = Math.round((used / total) * 100);
    return {
      usedKb: (used / 1024).toFixed(1),
      totalKb: (total / 1024).toFixed(1),
      pct,
    };
  };

  const formatUptime = (sec: number | undefined) => {
    if (sec == null) return '--';
    const days = Math.floor(sec / 86400);
    const hrs = Math.floor((sec % 86400) / 3600);
    const mins = Math.floor((sec % 3600) / 60);
    const secs = sec % 60;
    if (days > 0) return `${days}d ${hrs}h ${mins}m ${secs}s`;
    if (hrs > 0) return `${hrs}h ${mins}m ${secs}s`;
    if (mins > 0) return `${mins}m ${secs}s`;
    return `${secs}s`;
  };

  return (
    <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
      {/* Network & Hardware */}
      <Card bordered>
        <CardBody>
          <CardTitle>Network & Hardware</CardTitle>
          <div class="overflow-x-auto">
            <table class="table table-sm">
              <tbody>
                <tr class="border-b border-base-content/10">
                  <td class="text-base-content/70">System State</td>
                  <td class="text-right font-mono font-semibold text-primary">
                    {props.status?.state || '--'}
                  </td>
                </tr>
                <tr class="border-b border-base-content/10">
                  <td class="text-base-content/70">Uptime</td>
                  <td class="text-right font-mono">{formatUptime(props.status?.uptime_sec)}</td>
                </tr>
                <tr class="border-b border-base-content/10">
                  <td class="text-base-content/70">IP Address</td>
                  <td class="text-right font-mono">{props.status?.ip || 'N/A'}</td>
                </tr>
                <tr class="border-b border-base-content/10">
                  <td class="text-base-content/70">MAC Address</td>
                  <td class="text-right font-mono">{props.status?.mac || 'N/A'}</td>
                </tr>
                <tr class="border-b border-base-content/10">
                  <td class="text-base-content/70">Wi-Fi SSID</td>
                  <td class="text-right font-mono">{props.status?.wifi_ssid || 'N/A'}</td>
                </tr>
                <tr class="border-b border-base-content/10">
                  <td class="text-base-content/70">Wi-Fi RSSI</td>
                  <td class="text-right font-mono">
                    {props.status?.wifi_rssi_dbm != null
                      ? `${props.status.wifi_rssi_dbm} dBm`
                      : 'N/A'}
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
        </CardBody>
      </Card>

      {/* Firmware & Memory */}
      <Card bordered>
        <CardBody>
          <CardTitle>Firmware & Memory</CardTitle>
          <div class="overflow-x-auto">
            <table class="table table-sm">
              <tbody>
                <tr class="border-b border-base-content/10">
                  <td class="text-base-content/70">Active Slot</td>
                  <td class="text-right font-mono font-bold">
                    {props.status?.active_slot || '--'}
                  </td>
                </tr>
                <tr class="border-b border-base-content/10">
                  <td class="text-base-content/70">Slot Valid</td>
                  <td class="text-right">
                    <Badge variant={props.status?.slot_valid ? 'success' : 'error'}>
                      {props.status?.slot_valid ? 'Valid' : 'Pending'}
                    </Badge>
                  </td>
                </tr>
                <tr class="border-b border-base-content/10">
                  <td class="text-base-content/70">Heap Memory</td>
                  <td class="text-right font-mono">
                    <div>
                      {heapInfo().usedKb} / {heapInfo().totalKb} KB ({heapInfo().pct}%)
                    </div>
                    <progress
                      class="progress progress-primary w-28 h-2 mt-1"
                      value={heapInfo().pct}
                      max="100"
                    />
                  </td>
                </tr>
                <tr class="border-b border-base-content/10">
                  <td class="text-base-content/70">Version</td>
                  <td class="text-right font-mono">{props.status?.version || '--'}</td>
                </tr>
                <tr class="border-b border-base-content/10">
                  <td class="text-base-content/70">Git Commit</td>
                  <td class="text-right font-mono text-xs">{props.status?.git_commit || '--'}</td>
                </tr>
                <tr class="border-b border-base-content/10">
                  <td class="text-base-content/70">Build Date</td>
                  <td class="text-right font-mono text-xs">{props.status?.build_date || '--'}</td>
                </tr>
              </tbody>
            </table>
          </div>
        </CardBody>
      </Card>
    </div>
  );
};
