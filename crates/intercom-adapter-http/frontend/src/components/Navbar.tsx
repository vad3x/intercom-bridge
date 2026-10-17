import { Moon, Shield, Sun, Wifi } from 'lucide-solid';
import { type Component, createSignal, onMount } from 'solid-js';
import type { SystemDiagnostics } from '../types';
import { Badge } from './ui/Badge';

export interface NavbarProps {
  status: SystemDiagnostics | null | undefined;
}

export const Navbar: Component<NavbarProps> = (props) => {
  const [theme, setTheme] = createSignal<'night' | 'light'>('night');

  onMount(() => {
    const saved = (localStorage.getItem('intercom_theme') as 'night' | 'light') || 'night';
    setTheme(saved);
    document.documentElement.setAttribute('data-theme', saved);
  });

  const toggleTheme = () => {
    const next = theme() === 'night' ? 'light' : 'night';
    setTheme(next);
    document.documentElement.setAttribute('data-theme', next);
    try {
      localStorage.setItem('intercom_theme', next);
    } catch {
      // Ignore if localStorage is restricted
    }
  };

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

        <button
          type="button"
          class="btn btn-ghost btn-sm btn-circle"
          onClick={toggleTheme}
          aria-label="Toggle theme"
          title={`Switch to ${theme() === 'night' ? 'light' : 'night'} mode`}
        >
          {theme() === 'night' ? (
            <Sun class="w-4 h-4 text-warning" />
          ) : (
            <Moon class="w-4 h-4 text-primary" />
          )}
        </button>
      </div>
    </header>
  );
};
