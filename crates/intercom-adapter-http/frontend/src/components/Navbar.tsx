import { Palette, Shield, Wifi } from 'lucide-solid';
import { type Component, For, onMount } from 'solid-js';
import type { SystemDiagnostics } from '../types';
import { Badge } from './ui/Badge';

export interface NavbarProps {
  status: SystemDiagnostics | null | undefined;
}

const THEMES = ['dark', 'night', 'synthwave', 'dracula', 'black', 'light', 'corporate', 'winter'];

export const Navbar: Component<NavbarProps> = (props) => {
  const currentTheme = () => document.documentElement.getAttribute('data-theme') || 'dark';

  const setTheme = (theme: string) => {
    document.documentElement.setAttribute('data-theme', theme);
    localStorage.setItem('theme', theme);
  };

  onMount(() => {
    const saved = localStorage.getItem('theme');
    if (saved) {
      document.documentElement.setAttribute('data-theme', saved);
    } else {
      document.documentElement.setAttribute('data-theme', 'dark');
    }
  });

  const stateBadgeVariant = () => {
    const state = props.status?.state;
    switch (state) {
      case 'Operational':
        return 'success';
      case 'Commissioning':
      case 'WifiConnecting':
        return 'info';
      case 'Updating':
        return 'warning';
      case 'Error':
      case 'FactoryReset':
        return 'error';
      default:
        return 'neutral';
    }
  };

  return (
    <header class="navbar bg-base-100 border-b border-base-content/10 px-4 sm:px-6">
      <div class="navbar-start flex items-center gap-2">
        <Shield class="w-6 h-6 text-primary" />
        <span class="font-bold text-lg tracking-tight">Intercom Bridge</span>
        <Badge variant={stateBadgeVariant()} class="ml-2 font-mono">
          {props.status?.state || 'Connecting...'}
        </Badge>
      </div>

      <div class="navbar-end flex items-center gap-3">
        {props.status?.wifi_rssi_dbm != null && (
          <div class="hidden sm:flex items-center gap-1.5 text-xs font-mono text-base-content/70">
            <Wifi class="w-3.5 h-3.5" />
            <span>{props.status.wifi_rssi_dbm} dBm</span>
          </div>
        )}

        {props.status?.ip && (
          <span class="hidden md:inline text-xs font-mono px-2 py-0.5 rounded bg-base-200 text-base-content/80">
            {props.status.ip}
          </span>
        )}

        <div class="dropdown dropdown-end">
          <button type="button" class="btn btn-ghost btn-sm btn-square" aria-label="Theme selector">
            <Palette class="w-4 h-4" />
          </button>
          <ul class="dropdown-content menu bg-base-200 rounded-box z-50 w-36 p-2 shadow-lg border border-base-content/10">
            <For each={THEMES}>
              {(t) => (
                <li>
                  <button
                    type="button"
                    class={`capitalize text-xs font-semibold ${currentTheme() === t ? 'active' : ''}`}
                    onClick={() => setTheme(t)}
                  >
                    {t}
                  </button>
                </li>
              )}
            </For>
          </ul>
        </div>
      </div>
    </header>
  );
};
