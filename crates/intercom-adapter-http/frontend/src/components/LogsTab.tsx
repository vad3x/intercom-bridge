import { RotateCw } from 'lucide-solid';
import type { Component } from 'solid-js';
import { createSignal, onMount } from 'solid-js';
import { fetchLogs } from '../api';
import { Card, CardBody } from './ui/Card';

export const LogsTab: Component = () => {
  const [logs, setLogs] = createSignal<string>('Loading logs...');
  const [loading, setLoading] = createSignal<boolean>(false);

  const load = async () => {
    setLoading(true);
    try {
      const text = await fetchLogs();
      setLogs(text);
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      setLogs(`Failed to fetch logs: ${msg}`);
    } finally {
      setLoading(false);
    }
  };

  onMount(() => {
    load();
  });

  return (
    <Card bordered>
      <CardBody>
        <div class="flex items-center justify-between mb-3">
          <h3 class="text-sm font-semibold tracking-wider uppercase text-base-content/60 m-0">
            Live System Logs
          </h3>
          <button
            type="button"
            class="btn btn-sm btn-outline gap-2"
            onClick={load}
            disabled={loading()}
          >
            <RotateCw class={`w-4 h-4 ${loading() ? 'animate-spin' : ''}`} />
            Refresh Logs
          </button>
        </div>

        <pre class="bg-base-300 p-4 rounded-lg font-mono text-xs overflow-x-auto max-h-[500px] border border-base-content/10 leading-relaxed text-base-content/90 whitespace-pre-wrap select-text">
          {logs()}
        </pre>
      </CardBody>
    </Card>
  );
};
