import { Copy, Pause, Play, Trash2 } from 'lucide-solid';
import { type Component, createSignal, onCleanup, onMount } from 'solid-js';
import { fetchLogs } from '../api';
import { Card, CardBody, CardTitle } from './ui/Card';

export const LogsTab: Component = () => {
  const [logs, setLogs] = createSignal('');
  const [filter, setFilter] = createSignal('');
  const [isPaused, setIsPaused] = createSignal(false);
  const [copied, setCopied] = createSignal(false);

  let logContainerRef: HTMLPreElement | undefined;
  let pollInterval: ReturnType<typeof setInterval> | null = null;

  const updateLogs = async () => {
    if (isPaused() || (typeof document !== 'undefined' && document.hidden)) return;
    try {
      const text = await fetchLogs();
      setLogs(text);
      if (logContainerRef) {
        logContainerRef.scrollTop = logContainerRef.scrollHeight;
      }
    } catch {
      // Ignore transient fetch failure
    }
  };

  onMount(() => {
    updateLogs();
    pollInterval = setInterval(updateLogs, 2500);
  });

  onCleanup(() => {
    if (pollInterval) clearInterval(pollInterval);
  });

  const filteredLogs = () => {
    const raw = logs();
    const query = filter().toLowerCase();
    if (!query) return raw;
    return raw
      .split('\n')
      .filter((line) => line.toLowerCase().includes(query))
      .join('\n');
  };

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(logs());
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // Fallback
    }
  };

  return (
    <Card bordered class="h-[600px] flex flex-col">
      <CardBody class="flex-1 flex flex-col p-4">
        <div class="flex flex-wrap items-center justify-between gap-2 mb-3">
          <CardTitle>System Logs</CardTitle>

          <div class="flex items-center gap-2 flex-wrap">
            <input
              type="text"
              placeholder="Filter logs..."
              class="input input-sm input-bordered w-36 sm:w-48 font-mono text-xs"
              value={filter()}
              onInput={(e) => setFilter(e.currentTarget.value)}
            />

            <button
              type="button"
              class={`btn btn-sm ${isPaused() ? 'btn-warning' : 'btn-ghost'}`}
              onClick={() => setIsPaused(!isPaused())}
              title={isPaused() ? 'Resume live scrolling' : 'Pause scrolling'}
            >
              {isPaused() ? <Play class="w-4 h-4" /> : <Pause class="w-4 h-4" />}
            </button>

            <button
              type="button"
              class="btn btn-sm btn-ghost"
              onClick={handleCopy}
              title="Copy all logs"
            >
              <Copy class="w-4 h-4" />
              <span class="hidden sm:inline text-xs">{copied() ? 'Copied!' : 'Copy'}</span>
            </button>

            <button
              type="button"
              class="btn btn-sm btn-ghost text-error"
              onClick={() => setLogs('')}
              title="Clear log display"
            >
              <Trash2 class="w-4 h-4" />
            </button>
          </div>
        </div>

        <pre
          ref={logContainerRef}
          class="flex-1 bg-base-300 text-base-content p-3 rounded font-mono text-xs overflow-y-auto whitespace-pre-wrap break-all select-text"
        >
          {filteredLogs() || 'No logs received.'}
        </pre>
      </CardBody>
    </Card>
  );
};
