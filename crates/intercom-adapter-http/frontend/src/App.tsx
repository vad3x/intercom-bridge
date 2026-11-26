import {
  Badge,
  DashboardShell,
  type EcosystemStrategy,
  LastFailureCard,
  MatterEcosystemPanel,
  Toast,
} from '@embedded-device-foundation/dashboard-core';
import { DoorClosed, Layers } from 'lucide-solid';
import { createSignal, onCleanup, onMount } from 'solid-js';
import { clearLastFailure, fetchLastFailure, fetchLogs } from './api';
import { IntercomControls } from './components/IntercomControls';
import { createIntercomStore } from './store/intercomStore';

export function App() {
  const store = createIntercomStore();
  const [lastFailure, setLastFailure] = createSignal<string>('');
  const [isClearingFailure, setIsClearingFailure] = createSignal(false);
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
        variant: 'neutral' as const,
        dotClass: 'bg-base-content/30 animate-pulse',
        text: 'connecting',
      };
    }
    if (store.status()?.device.state.door_locked) {
      return { variant: 'success' as const, dotClass: 'bg-success', text: 'locked' };
    }
    return { variant: 'warning' as const, dotClass: 'bg-warning', text: 'unlocked' };
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

  const handleDurationChange = async (seconds: number) => {
    try {
      await store.updateDoorUnlockDuration(seconds);
      showToast(`Unlock duration set to ${seconds}s`, 'success');
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : 'Failed to update duration';
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

  const handleReboot = async () => {
    try {
      await store.rebootDevice();
      showToast('Device reboot initiated...', 'info');
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : 'Failed to reboot device';
      showToast(msg, 'error');
    }
  };

  const handleFactoryReset = async () => {
    try {
      await store.factoryResetDevice();
      showToast('Factory reset initiated. Device will reboot into setup mode...', 'info');
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : 'Failed to factory reset';
      showToast(msg, 'error');
    }
  };

  const ecosystemStrategies: EcosystemStrategy[] = [
    {
      id: 'matter',
      label: 'Matter over Wi-Fi',
      icon: Layers,
      Component: (props) => (
        <MatterEcosystemPanel
          ecosystem={props.ecosystem}
          subscriptionRows={[
            { label: 'Door Lock', propertyKey: 'door_lock_subscribed' },
            { label: 'Chime / Ring', propertyKey: 'ring_sensor_subscribed' },
          ]}
          endpointIcon={(endpoint) => {
            switch (endpoint.clusters?.[0]?.name) {
              case 'DoorLock':
                return '🔒 ';
              case 'OccupancySensing':
                return '🔔 ';
              default:
                return '';
            }
          }}
          onResetMatter={handleResetMatter}
          isResettingMatter={store.isResettingMatter()}
        />
      ),
    },
  ];

  const refreshLastFailure = async () => {
    try {
      setLastFailure(await fetchLastFailure());
    } catch {
      // Ignore transient poll errors
    }
  };

  const handleClearLastFailure = async () => {
    setIsClearingFailure(true);
    try {
      await clearLastFailure();
      setLastFailure('');
      showToast('Persisted failure record cleared', 'success');
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : 'Failed to clear failure record';
      showToast(msg, 'error');
    } finally {
      setIsClearingFailure(false);
    }
  };

  onMount(() => {
    store.poll();
    void refreshLastFailure();
    const interval = setInterval(store.poll, 2500);
    const failureInterval = setInterval(refreshLastFailure, 30000);
    onCleanup(() => {
      clearInterval(interval);
      clearInterval(failureInterval);
    });
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
        <div class="space-y-4">
          <IntercomControls
            doorLocked={store.status()?.device.state.door_locked}
            doorUnlockDurationSec={store.doorUnlockDurationSec()}
            isUnlocking={store.isUnlocking()}
            isRinging={store.isRinging()}
            onUnlock={handleUnlock}
            onRing={handleRing}
            onDurationChange={handleDurationChange}
          />
          <LastFailureCard
            lastFailure={lastFailure()}
            isClearingFailure={isClearingFailure()}
            onClearLastFailure={handleClearLastFailure}
          />
        </div>
      }
      status={store.status()}
      themeStorageKey="intercom_theme"
      tabOrder={['controls', 'ecosystem', 'diagnostics', 'logs']}
      ecosystemStrategies={ecosystemStrategies}
      loadLogs={fetchLogs}
      onReboot={handleReboot}
      onFactoryReset={handleFactoryReset}
      toast={<Toast message={toastMsg()} type={toastType()} />}
    />
  );
}
