import { Activity, Radio, Sliders, Terminal } from 'lucide-solid';
import { type Component, For, Match, Switch, createSignal, onCleanup, onMount } from 'solid-js';
import { DiagnosticsTab } from './components/DiagnosticsTab';
import { EcosystemTab } from './components/EcosystemTab';
import { IntercomControls } from './components/IntercomControls';
import { LogsTab } from './components/LogsTab';
import { Navbar } from './components/Navbar';
import { Toast } from './components/Toast';
import { createIntercomStore } from './store/intercomStore';

type TabId = 'controls' | 'ecosystem' | 'diagnostics' | 'logs';

export function App() {
  const store = createIntercomStore();
  const [activeTab, setActiveTab] = createSignal<TabId>('controls');
  const [toastMsg, setToastMsg] = createSignal<string | null>(null);
  const [toastType, setToastType] = createSignal<'info' | 'success' | 'error'>('info');

  let toastTimer: ReturnType<typeof setTimeout> | null = null;

  const showToast = (msg: string, type: 'info' | 'success' | 'error' = 'info') => {
    setToastMsg(msg);
    setToastType(type);
    if (toastTimer) clearTimeout(toastTimer);
    toastTimer = setTimeout(() => {
      setToastMsg(null);
    }, 2500);
  };

  const handleUnlock = async () => {
    try {
      await store.unlockDoor();
      showToast('Unlock command sent', 'success');
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : 'Failed to unlock';
      showToast(msg, 'error');
    }
  };

  const handleRing = async () => {
    try {
      await store.testRing();
      showToast('Test ring triggered', 'success');
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : 'Failed to trigger ring';
      showToast(msg, 'error');
    }
  };

  onMount(() => {
    store.poll();
    const interval = setInterval(store.poll, 2500);
    onCleanup(() => clearInterval(interval));
  });

  const tabs: {
    id: TabId;
    label: string;
    icon: Component<{ class?: string }>;
  }[] = [
    { id: 'controls', label: 'Controls', icon: Sliders },
    { id: 'ecosystem', label: 'Ecosystem', icon: Radio },
    { id: 'diagnostics', label: 'Diagnostics', icon: Activity },
    { id: 'logs', label: 'Logs', icon: Terminal },
  ];

  return (
    <div class="min-h-screen flex flex-col bg-base-300">
      <Navbar status={store.status()} />

      {/* Tabs Navigation */}
      <nav class="bg-base-200 border-b border-base-content/10 px-4 sm:px-6">
        <div class="flex gap-1 overflow-x-auto max-w-4xl mx-auto">
          <For each={tabs}>
            {(t) => {
              const Icon = t.icon;
              const isActive = () => activeTab() === t.id;
              return (
                <button
                  type="button"
                  class={`flex items-center gap-2 py-3 px-4 font-semibold text-sm border-b-2 transition-colors whitespace-nowrap ${
                    isActive()
                      ? 'border-primary text-primary'
                      : 'border-transparent text-base-content/60 hover:text-base-content'
                  }`}
                  onClick={() => setActiveTab(t.id)}
                >
                  <Icon class="w-4 h-4" />
                  {t.label}
                </button>
              );
            }}
          </For>
        </div>
      </nav>

      {/* Main Content Area */}
      <main class="flex-1 max-w-4xl w-full mx-auto p-4 sm:p-6">
        <Switch>
          <Match when={activeTab() === 'controls'}>
            <IntercomControls
              doorLocked={store.status()?.door_locked}
              isUnlocking={store.isUnlocking()}
              isRinging={store.isRinging()}
              onUnlock={handleUnlock}
              onRing={handleRing}
            />
          </Match>
          <Match when={activeTab() === 'ecosystem'}>
            <EcosystemTab ecosystem={store.status()?.active_ecosystem} />
          </Match>
          <Match when={activeTab() === 'diagnostics'}>
            <DiagnosticsTab status={store.status()} />
          </Match>
          <Match when={activeTab() === 'logs'}>
            <LogsTab />
          </Match>
        </Switch>
      </main>

      <Toast message={toastMsg()} type={toastType()} />
    </div>
  );
}
