import { Badge, DashboardShell, Toast } from '@embedded-device-foundation/dashboard-core';
import { DoorClosed } from 'lucide-solid';
import { createSignal, onCleanup, onMount } from 'solid-js';
import { fetchLogs } from './api';
import { IntercomControls } from './components/IntercomControls';
import { IntercomEcosystemSupplementalPanel } from './components/IntercomEcosystemSupplementalPanel';
import { createIntercomStore } from './store/intercomStore';

export function App() {
  const store = createIntercomStore();
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

  const doorStatus = () => {
    if (!store.status()) {
      return {
        variant: 'warning' as const,
        dotClass: 'bg-warning animate-pulse',
        text: 'connecting',
      };
    }
    if (store.status()?.door_locked) {
      return { variant: 'success' as const, dotClass: 'bg-success animate-pulse', text: 'locked' };
    }
    return { variant: 'warning' as const, dotClass: 'bg-warning animate-pulse', text: 'unlocked' };
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

  const handleResetMatter = async () => {
    try {
      await store.resetMatter();
      showToast('Matter KVS reset initiated. Rebooting...', 'info');
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : 'Failed to reset Matter KVS';
      showToast(msg, 'error');
    }
  };

  onMount(() => {
    store.poll();
    const interval = setInterval(store.poll, 2500);
    onCleanup(() => clearInterval(interval));
  });

  return (
    <DashboardShell
      title="Intercom Bridge"
      brandIcon={<DoorClosed class="w-5 h-5" />}
      badges={
        <Badge variant={doorStatus().variant}>
          <span class={`w-2 h-2 rounded-full ${doorStatus().dotClass}`} />
          <span class="lowercase">door:</span>
          <span class="normal-case">{doorStatus().text}</span>
        </Badge>
      }
      controlTab={
        <IntercomControls
          doorLocked={store.status()?.door_locked}
          isUnlocking={store.isUnlocking()}
          isRinging={store.isRinging()}
          onUnlock={handleUnlock}
          onRing={handleRing}
        />
      }
      status={store.status()}
      themeStorageKey="intercom_theme"
      subscriptionRows={[
        { label: '🔒 Door Lock (EP1)', propertyKey: 'door_lock_subscribed' },
        { label: '🔔 Chime / Ring (EP2)', propertyKey: 'ring_sensor_subscribed' },
      ]}
      tabOrder={['controls', 'ecosystem', 'diagnostics', 'logs']}
      endpointIcon={(endpoint) => (endpoint.id === 1 ? '🔒 ' : endpoint.id === 2 ? '🔔 ' : '')}
      supplementalPanel={<IntercomEcosystemSupplementalPanel />}
      onResetMatter={handleResetMatter}
      isResettingMatter={store.isResettingMatter()}
      loadLogs={fetchLogs}
      toast={<Toast message={toastMsg()} type={toastType()} />}
    />
  );
}
