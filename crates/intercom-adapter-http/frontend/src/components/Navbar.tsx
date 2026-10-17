import { DoorClosed, Moon, Sun } from 'lucide-solid';
import { type Component, createSignal, onMount } from 'solid-js';
import type { SystemDiagnostics } from '../types';
import { Badge } from './ui/Badge';

export interface NavbarProps {
  status: SystemDiagnostics | null | undefined;
}

export const Navbar: Component<NavbarProps> = (props) => {
  const eco = () => props.status?.active_ecosystem;

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

  const doorStatus = () => {
    if (!props.status) {
      return {
        variant: 'warning' as const,
        dotClass: 'bg-warning animate-pulse',
        text: 'connecting',
      };
    }
    if (props.status.door_locked) {
      return { variant: 'success' as const, dotClass: 'bg-success animate-pulse', text: 'locked' };
    }
    return { variant: 'warning' as const, dotClass: 'bg-warning animate-pulse', text: 'unlocked' };
  };

  const ecoStatus = () => {
    if (!eco()) {
      return { variant: 'error' as const, dotClass: 'bg-error', name: 'none' };
    }
    if (eco()?.paired) {
      return {
        variant: 'success' as const,
        dotClass: 'bg-success animate-pulse',
        name: eco()?.type || 'matter',
      };
    }
    return {
      variant: 'warning' as const,
      dotClass: 'bg-warning',
      name: eco()?.type || 'matter',
    };
  };

  return (
    <header class="navbar bg-base-200 border-b border-base-content/10 px-4 sm:px-6 justify-between flex-wrap gap-3">
      <div class="flex items-center gap-3">
        <div class="p-2 rounded-lg bg-primary/15 text-primary">
          <DoorClosed class="w-5 h-5" />
        </div>
        <span class="font-bold text-lg tracking-tight text-base-content">Intercom Bridge</span>
      </div>

      <div class="flex items-center flex-wrap gap-2.5">
        {/* Intercom Door Status Badge */}
        <Badge variant={doorStatus().variant}>
          <span class={`w-2 h-2 rounded-full ${doorStatus().dotClass}`} />
          <span class="lowercase">door:</span>
          <span class="normal-case">{doorStatus().text}</span>
        </Badge>

        {/* Ecosystem Status Badge */}
        <Badge variant={ecoStatus().variant}>
          <span class={`w-2 h-2 rounded-full ${ecoStatus().dotClass}`} />
          <span class="lowercase">ecosystem:</span>
          <span class="normal-case">{ecoStatus().name}</span>
        </Badge>

        {/* Light / Dark Theme Toggle Button */}
        <button
          type="button"
          class="btn btn-ghost btn-circle btn-sm text-base-content/70 hover:text-base-content ml-1"
          onClick={toggleTheme}
          aria-label={theme() === 'night' ? 'Switch to light theme' : 'Switch to dark theme'}
          title={theme() === 'night' ? 'Switch to light mode' : 'Switch to dark mode'}
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
