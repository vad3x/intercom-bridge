# Intercom Bridge Web Dashboard Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replicate the modern, responsive Web Dashboard architecture from `hvac-bridge` for `intercom-matter`, embedding a compiled, self-contained single-page application into flash memory via `intercom-adapter-http`.

**Architecture:** A lightweight Solid.js application using DaisyUI, Tailwind CSS, and Lucide Solid icons built with Bun and Vite (`vite-plugin-singlefile`) into a single `assets/dashboard.html`. The Rust `build.rs` script gzips this asset into `dashboard.html.gz` for zero-allocation embedding into the ESP32-C6 firmware binary, served directly over HTTP with `Content-Encoding: gzip`.

**Tech Stack:** Bun, Solid.js, TypeScript, Tailwind CSS, DaisyUI, Lucide Solid, Vite, Rust (`no_std`, `picoserve`, `flate2`).

## Global Constraints

- Replicate the exact design, build pipeline, and developer workflow from `hvac-bridge/crates/hvac-adapter-http/frontend/`.
- Single-page bundled artifact in `crates/intercom-adapter-http/assets/dashboard.html` must remain under 50 KiB gzipped (< 20 KiB target).
- Preserve existing REST API endpoints: `GET /`, `GET /api/status`, `GET /api/logs`, `POST /api/action/unlock`, `POST /api/action/ring`, `POST /api/v1/ota/flash`.
- All tests must adhere to Arrange-Act-Assert structure.
- Public items before private items in Rust files.

---

### Task 1: Frontend Project Tooling & Scaffolding

**Files:**
- Create: `crates/intercom-adapter-http/frontend/package.json`
- Create: `crates/intercom-adapter-http/frontend/tsconfig.json`
- Create: `crates/intercom-adapter-http/frontend/vite.config.ts`
- Create: `crates/intercom-adapter-http/frontend/postcss.config.js`
- Create: `crates/intercom-adapter-http/frontend/biome.json`
- Create: `crates/intercom-adapter-http/frontend/.mise.toml`
- Create: `crates/intercom-adapter-http/frontend/README.md`
- Modify: `mise.toml`

**Interfaces:**
- Produces: Bun build toolchain and configuration for compiling the dashboard to `../assets/dashboard.html`.

- [ ] **Step 1: Update root `mise.toml` and create `crates/intercom-adapter-http/frontend/.mise.toml`**

Add `bun = "latest"` so `mise` provides the Bun runtime.

`mise.toml`:
```toml
[tools]
rust = "latest"
bun = "latest"
```

`crates/intercom-adapter-http/frontend/.mise.toml`:
```toml
[tools]
bun = "latest"
```

- [ ] **Step 2: Create `package.json`, `tsconfig.json`, `postcss.config.js`, `biome.json`, and `vite.config.ts`**

`crates/intercom-adapter-http/frontend/package.json`:
```json
{
  "name": "intercom-dashboard",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "bun --bun vite",
    "build": "bun --bun vite build && mv ../assets/index.html ../assets/dashboard.html",
    "preview": "bun --bun vite preview",
    "lint": "biome lint .",
    "format": "biome format --write .",
    "check": "biome check --write ."
  },
  "dependencies": {
    "clsx": "^2.1.1",
    "lucide-solid": "^1.16.0",
    "solid-js": "^1.9.4",
    "tailwind-merge": "^3.0.1"
  },
  "devDependencies": {
    "@biomejs/biome": "^1.9.4",
    "autoprefixer": "^10.4.20",
    "daisyui": "^4.12.23",
    "postcss": "^8.5.1",
    "tailwindcss": "^3.4.17",
    "typescript": "^5.7.3",
    "vite": "^6.0.11",
    "vite-plugin-singlefile": "^2.1.0",
    "vite-plugin-solid": "^2.11.6"
  }
}
```

`crates/intercom-adapter-http/frontend/tsconfig.json`:
```json
{
  "compilerOptions": {
    "target": "ESNext",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "allowSyntheticDefaultImports": true,
    "esModuleInterop": true,
    "jsx": "preserve",
    "jsxImportSource": "solid-js",
    "types": ["vite/client"],
    "strict": true,
    "noEmit": true,
    "isolatedModules": true,
    "baseUrl": ".",
    "paths": {
      "~/*": ["src/*"]
    }
  },
  "include": ["src", "vite.config.ts"]
}
```

`crates/intercom-adapter-http/frontend/postcss.config.js`:
```javascript
export default {
  plugins: {
    tailwindcss: {},
    autoprefixer: {},
  },
};
```

`crates/intercom-adapter-http/frontend/biome.json`:
```json
{
  "$schema": "https://biomejs.dev/schemas/1.9.4/schema.json",
  "organizeImports": {
    "enabled": true
  },
  "linter": {
    "enabled": true,
    "rules": {
      "recommended": true
    }
  },
  "formatter": {
    "enabled": true,
    "indentStyle": "space",
    "indentWidth": 2,
    "lineWidth": 100
  },
  "javascript": {
    "formatter": {
      "quoteStyle": "single",
      "semicolons": "always"
    }
  }
}
```

`crates/intercom-adapter-http/frontend/vite.config.ts`:
```typescript
import path from 'node:path';
import { defineConfig } from 'vite';
import { viteSingleFile } from 'vite-plugin-singlefile';
import solidPlugin from 'vite-plugin-solid';

export default defineConfig({
  plugins: [solidPlugin(), viteSingleFile()],
  resolve: {
    alias: {
      '~': path.resolve(__dirname, './src'),
    },
  },
  build: {
    outDir: '../assets',
    emptyOutDir: false,
    cssCodeSplit: false,
    assetsInlineLimit: 100000000,
    rollupOptions: {
      output: {
        inlineDynamicImports: true,
      },
    },
  },
  server: {
    port: 5173,
    proxy: {
      '/api': {
        target: 'http://intercom-bridge.local:8080',
        changeOrigin: true,
      },
    },
  },
});
```

`crates/intercom-adapter-http/frontend/README.md`:
```markdown
# Intercom Bridge Web Dashboard (Frontend)

Modern, lightweight, responsive web dashboard built with **Solid.js**, **DaisyUI**, and **Tailwind CSS**, managed with **Bun**.

Bundles into a single, self-contained `assets/dashboard.html` (< 15-20 KB gzipped) embedded directly in the ESP32-C6 firmware binary.

## Development Workflow

### 1. Install Dependencies
```bash
bun install
```

### 2. Start Local Development Server
```bash
bun run dev
```
Starts hot-reloading dev server at `http://localhost:5173`.

### 3. Build for Firmware Release
```bash
bun run build
```
This builds and inlines all HTML, CSS, JavaScript, and SVG icons into `../assets/dashboard.html`.
```

- [ ] **Step 3: Run `mise exec -- bun install` inside `crates/intercom-adapter-http/frontend`**

Run: `cd crates/intercom-adapter-http/frontend && mise exec -- bun install`
Expected: Dependencies installed successfully, creating `bun.lock`.

- [ ] **Step 4: Commit**

```bash
git add mise.toml crates/intercom-adapter-http/frontend
git commit -m "feat(frontend): scaffold bun, vite, solid, daisyui tooling for intercom dashboard"
```

---

### Task 2: TypeScript Data Models, API Client & Reactive Store

**Files:**
- Create: `crates/intercom-adapter-http/frontend/src/types.ts`
- Create: `crates/intercom-adapter-http/frontend/src/api.ts`
- Create: `crates/intercom-adapter-http/frontend/src/store/intercomStore.ts`

**Interfaces:**
- Consumes: REST API contract from `crates/intercom-adapter-http/src/router.rs` (`/api/status`, `/api/logs`, `/api/action/unlock`, `/api/action/ring`).
- Produces: `createIntercomStore()` returning `{ status, isUnlocking, unlockDoor, testRing, poll }`.

- [ ] **Step 1: Write `src/types.ts`**

`crates/intercom-adapter-http/frontend/src/types.ts`:
```typescript
export interface ActiveEcosystem {
  type: string;
  paired: boolean;
  properties: {
    door_lock_subscribed?: boolean;
    ring_sensor_subscribed?: boolean;
    [key: string]: boolean | number | string | undefined;
  };
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
```

- [ ] **Step 2: Write `src/api.ts`**

`crates/intercom-adapter-http/frontend/src/api.ts`:
```typescript
import type { SystemDiagnostics } from './types';

export async function fetchStatus(): Promise<SystemDiagnostics> {
  const res = await fetch('/api/status');
  if (!res.ok) {
    throw new Error(`HTTP error ${res.status}`);
  }
  return res.json();
}

export async function fetchLogs(): Promise<string> {
  const res = await fetch('/api/logs');
  if (!res.ok) {
    throw new Error(`HTTP error ${res.status}`);
  }
  return res.text();
}

export async function triggerUnlock(): Promise<void> {
  const res = await fetch('/api/action/unlock', { method: 'POST' });
  if (!res.ok) {
    throw new Error(`Unlock failed: HTTP ${res.status}`);
  }
}

export async function triggerRing(): Promise<void> {
  const res = await fetch('/api/action/ring', { method: 'POST' });
  if (!res.ok) {
    throw new Error(`Ring failed: HTTP ${res.status}`);
  }
}
```

- [ ] **Step 3: Write `src/store/intercomStore.ts`**

`crates/intercom-adapter-http/frontend/src/store/intercomStore.ts`:
```typescript
import { createSignal } from 'solid-js';
import { fetchStatus, triggerRing, triggerUnlock } from '../api';
import type { SystemDiagnostics } from '../types';

export function createIntercomStore() {
  const [status, setStatus] = createSignal<SystemDiagnostics | null>(null);
  const [isUnlocking, setIsUnlocking] = createSignal(false);
  const [isRinging, setIsRinging] = createSignal(false);

  const poll = async () => {
    try {
      const data = await fetchStatus();
      setStatus(data);
      // If hardware reports locked and unlocking intent expired, clear unlocking
      if (data.door_locked && isUnlocking()) {
        setIsUnlocking(false);
      }
    } catch {
      // Offline / server unreachable
    }
  };

  const unlockDoor = async () => {
    setIsUnlocking(true);
    try {
      await triggerUnlock();
      await poll();
    } catch (e) {
      setIsUnlocking(false);
      throw e;
    }
  };

  const testRing = async () => {
    setIsRinging(true);
    try {
      await triggerRing();
      setTimeout(() => setIsRinging(false), 3000);
    } catch (e) {
      setIsRinging(false);
      throw e;
    }
  };

  return {
    status,
    isUnlocking,
    isRinging,
    poll,
    unlockDoor,
    testRing,
  };
}
```

- [ ] **Step 4: Verify with Biome check**

Run: `cd crates/intercom-adapter-http/frontend && mise exec -- bun run check`
Expected: Biome passes with no errors.

- [ ] **Step 5: Commit**

```bash
git add crates/intercom-adapter-http/frontend/src/types.ts crates/intercom-adapter-http/frontend/src/api.ts crates/intercom-adapter-http/frontend/src/store/intercomStore.ts
git commit -m "feat(frontend): implement intercom api client, data types, and reactive store"
```

---

### Task 3: Base Styles, UI Components & Navigation Header

**Files:**
- Create: `crates/intercom-adapter-http/frontend/src/index.css`
- Create: `crates/intercom-adapter-http/frontend/src/components/ui/Badge.tsx`
- Create: `crates/intercom-adapter-http/frontend/src/components/ui/Card.tsx`
- Create: `crates/intercom-adapter-http/frontend/src/components/Toast.tsx`
- Create: `crates/intercom-adapter-http/frontend/src/components/Navbar.tsx`

**Interfaces:**
- Produces: `Badge`, `Card`, `Toast`, and `Navbar` components mirroring `hvac-bridge`.

- [ ] **Step 1: Write `src/index.css`**

`crates/intercom-adapter-http/frontend/src/index.css`:
```css
@tailwind base;
@tailwind components;
@tailwind utilities;

:root {
  color-scheme: dark light;
}

body {
  margin: 0;
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
}
```

- [ ] **Step 2: Write `src/components/ui/Badge.tsx` and `src/components/ui/Card.tsx`**

`crates/intercom-adapter-http/frontend/src/components/ui/Badge.tsx`:
```tsx
import type { JSX, ParentComponent } from 'solid-js';

export interface BadgeProps {
  variant?: 'primary' | 'secondary' | 'accent' | 'success' | 'warning' | 'error' | 'info' | 'neutral';
  size?: 'xs' | 'sm' | 'md' | 'lg';
  outline?: boolean;
  class?: string;
  children?: JSX.Element;
}

export const Badge: ParentComponent<BadgeProps> = (props) => {
  const variantClass = () => {
    switch (props.variant) {
      case 'primary': return 'badge-primary';
      case 'secondary': return 'badge-secondary';
      case 'accent': return 'badge-accent';
      case 'success': return 'badge-success';
      case 'warning': return 'badge-warning';
      case 'error': return 'badge-error';
      case 'info': return 'badge-info';
      default: return 'badge-neutral';
    }
  };

  const sizeClass = () => {
    switch (props.size) {
      case 'xs': return 'badge-xs';
      case 'sm': return 'badge-sm';
      case 'lg': return 'badge-lg';
      default: return 'badge-sm';
    }
  };

  return (
    <span
      class={`badge ${variantClass()} ${sizeClass()} ${props.outline ? 'badge-outline' : ''} ${props.class || ''}`}
    >
      {props.children}
    </span>
  );
};
```

`crates/intercom-adapter-http/frontend/src/components/ui/Card.tsx`:
```tsx
import type { JSX, ParentComponent } from 'solid-js';

export interface CardProps {
  bordered?: boolean;
  class?: string;
  children?: JSX.Element;
}

export const Card: ParentComponent<CardProps> = (props) => {
  return (
    <div
      class={`card bg-base-100 shadow-md ${props.bordered ? 'border border-base-content/10' : ''} ${props.class || ''}`}
    >
      {props.children}
    </div>
  );
};

export const CardBody: ParentComponent<{ class?: string; children?: JSX.Element }> = (props) => {
  return <div class={`card-body p-4 sm:p-6 ${props.class || ''}`}>{props.children}</div>;
};

export const CardTitle: ParentComponent<{ class?: string; children?: JSX.Element }> = (props) => {
  return (
    <h2 class={`card-title text-base sm:text-lg font-bold flex items-center gap-2 ${props.class || ''}`}>
      {props.children}
    </h2>
  );
};
```

- [ ] **Step 3: Write `src/components/Toast.tsx`**

`crates/intercom-adapter-http/frontend/src/components/Toast.tsx`:
```tsx
import type { Component } from 'solid-js';
import { Show } from 'solid-js';

export interface ToastProps {
  message: string | null;
  type?: 'info' | 'success' | 'error';
}

export const Toast: Component<ToastProps> = (props) => {
  const alertTypeClass = () => {
    switch (props.type) {
      case 'success': return 'alert-success';
      case 'error': return 'alert-error';
      default: return 'alert-info';
    }
  };

  return (
    <Show when={props.message}>
      <div class="toast toast-end toast-bottom z-50 p-4">
        <div class={`alert ${alertTypeClass()} shadow-lg text-sm font-semibold flex items-center`}>
          <span>{props.message}</span>
        </div>
      </div>
    </Show>
  );
};
```

- [ ] **Step 4: Write `src/components/Navbar.tsx`**

`crates/intercom-adapter-http/frontend/src/components/Navbar.tsx`:
```tsx
import { Shield, Palette, Wifi } from 'lucide-solid';
import { type Component, For, onMount } from 'solid-js';
import type { SystemDiagnostics } from '../types';
import { Badge } from './ui/Badge';

export interface NavbarProps {
  status: SystemDiagnostics | null | undefined;
}

const THEMES = [
  'dark',
  'night',
  'synthwave',
  'dracula',
  'black',
  'light',
  'corporate',
  'winter',
];

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
      case 'Operational': return 'success';
      case 'Commissioning':
      case 'WifiConnecting': return 'info';
      case 'Updating': return 'warning';
      case 'Error':
      case 'FactoryReset': return 'error';
      default: return 'neutral';
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
          <div tabIndex={0} role="button" class="btn btn-ghost btn-sm btn-square" aria-label="Theme selector">
            <Palette class="w-4 h-4" />
          </div>
          <ul
            tabIndex={0}
            class="dropdown-content menu bg-base-200 rounded-box z-50 w-36 p-2 shadow-lg border border-base-content/10"
          >
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
```

- [ ] **Step 5: Verify with Biome check**

Run: `cd crates/intercom-adapter-http/frontend && mise exec -- bun run check`
Expected: Biome passes with no errors.

- [ ] **Step 6: Commit**

```bash
git add crates/intercom-adapter-http/frontend/src/index.css crates/intercom-adapter-http/frontend/src/components
git commit -m "feat(frontend): add base styling, reusable card/badge/toast UI, and navbar"
```

---

### Task 4: Feature Tabs: IntercomControls, EcosystemTab, DiagnosticsTab, LogsTab & App Shell

**Files:**
- Create: `crates/intercom-adapter-http/frontend/src/components/IntercomControls.tsx`
- Create: `crates/intercom-adapter-http/frontend/src/components/EcosystemTab.tsx`
- Create: `crates/intercom-adapter-http/frontend/src/components/DiagnosticsTab.tsx`
- Create: `crates/intercom-adapter-http/frontend/src/components/LogsTab.tsx`
- Create: `crates/intercom-adapter-http/frontend/src/App.tsx`
- Create: `crates/intercom-adapter-http/frontend/src/main.tsx`
- Create: `crates/intercom-adapter-http/frontend/index.html`

**Interfaces:**
- Produces: Complete 4-tab interactive dashboard application ready for bundling.

- [ ] **Step 1: Write `src/components/IntercomControls.tsx`**

`crates/intercom-adapter-http/frontend/src/components/IntercomControls.tsx`:
```tsx
import { Bell, Lock, Unlock, Loader2, Sparkles } from 'lucide-solid';
import type { Component } from 'solid-js';
import { Card, CardBody, CardTitle } from './ui/Card';
import { Badge } from './ui/Badge';

export interface IntercomControlsProps {
  doorLocked: boolean | undefined;
  isUnlocking: boolean;
  isRinging: boolean;
  onUnlock: () => void;
  onRing: () => void;
}

export const IntercomControls: Component<IntercomControlsProps> = (props) => {
  const isLocked = () => !props.isUnlocking && (props.doorLocked ?? true);

  return (
    <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
      {/* Door Strike / Lock Control Card */}
      <Card bordered>
        <CardBody class="flex flex-col justify-between">
          <div>
            <div class="flex items-center justify-between mb-4">
              <CardTitle>Door Access</CardTitle>
              <Badge variant={isLocked() ? 'success' : 'warning'}>
                {props.isUnlocking ? 'Unlocking...' : isLocked() ? 'Locked' : 'Unlocked'}
              </Badge>
            </div>

            <div class="flex flex-col items-center justify-center py-6 text-center">
              <div
                class={`w-24 h-24 rounded-full flex items-center justify-center transition-all duration-300 ${
                  isLocked()
                    ? 'bg-success/15 text-success'
                    : 'bg-warning/20 text-warning animate-pulse'
                }`}
              >
                {props.isUnlocking ? (
                  <Loader2 class="w-12 h-12 animate-spin" />
                ) : isLocked() ? (
                  <Lock class="w-12 h-12" />
                ) : (
                  <Unlock class="w-12 h-12" />
                )}
              </div>
              <p class="mt-4 font-semibold text-lg">
                {props.isUnlocking
                  ? 'Actuating Door Strike...'
                  : isLocked()
                  ? 'Entrance Door Secured'
                  : 'Entrance Door Unlocked'}
              </p>
              <p class="text-xs text-base-content/60 mt-1">
                PhotoMOS Relay • Auto-relocks after strike pulse
              </p>
            </div>
          </div>

          <button
            type="button"
            class="btn btn-primary w-full gap-2 text-base font-bold shadow-md"
            disabled={props.isUnlocking}
            onClick={props.onUnlock}
          >
            {props.isUnlocking ? (
              <>
                <Loader2 class="w-5 h-5 animate-spin" />
                Unlocking...
              </>
            ) : (
              <>
                <Unlock class="w-5 h-5" />
                Unlock Door
              </>
            )}
          </button>
        </CardBody>
      </Card>

      {/* Doorbell Ring Status & Test Card */}
      <Card bordered>
        <CardBody class="flex flex-col justify-between">
          <div>
            <div class="flex items-center justify-between mb-4">
              <CardTitle>Doorbell Sensor</CardTitle>
              <Badge variant={props.isRinging ? 'warning' : 'neutral'}>
                {props.isRinging ? 'Ringing' : 'Idle'}
              </Badge>
            </div>

            <div class="flex flex-col items-center justify-center py-6 text-center">
              <div
                class={`w-24 h-24 rounded-full flex items-center justify-center transition-all duration-300 ${
                  props.isRinging
                    ? 'bg-warning/20 text-warning animate-bounce'
                    : 'bg-base-200 text-base-content/60'
                }`}
              >
                <Bell class="w-12 h-12" />
              </div>
              <p class="mt-4 font-semibold text-lg">
                {props.isRinging ? 'Doorbell Ring Detected!' : 'Doorbell Chime Idle'}
              </p>
              <p class="text-xs text-base-content/60 mt-1">
                Optocoupler Input • Bridges analog chime to Matter Occupancy
              </p>
            </div>
          </div>

          <button
            type="button"
            class="btn btn-secondary w-full gap-2 text-base font-bold shadow-md"
            disabled={props.isRinging}
            onClick={props.onRing}
          >
            <Sparkles class="w-5 h-5" />
            Test Doorbell Ring
          </button>
        </CardBody>
      </Card>
    </div>
  );
};
```

- [ ] **Step 2: Write `src/components/EcosystemTab.tsx`**

`crates/intercom-adapter-http/frontend/src/components/EcosystemTab.tsx`:
```tsx
import { Check } from 'lucide-solid';
import type { Component } from 'solid-js';
import type { ActiveEcosystem } from '../types';
import { Badge } from './ui/Badge';
import { Card, CardBody, CardTitle } from './ui/Card';

export interface EcosystemTabProps {
  ecosystem: ActiveEcosystem | null | undefined;
}

export const EcosystemTab: Component<EcosystemTabProps> = (props) => {
  const isPaired = () => props.ecosystem?.paired ?? false;
  const propsObj = () => props.ecosystem?.properties || {};

  const renderSubBadge = (val: boolean | undefined) => {
    if (val === true) {
      return <Badge variant="success">Subscribed</Badge>;
    }
    if (val === false) {
      return <Badge variant="warning">Waiting</Badge>;
    }
    return <Badge variant="neutral">Ready</Badge>;
  };

  return (
    <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
      {/* Matter Ecosystem Card */}
      <Card bordered>
        <CardBody>
          <div class="flex items-center justify-between mb-2">
            <CardTitle class="text-base font-bold text-base-content m-0">
              Matter over Wi-Fi
            </CardTitle>
            <Badge variant="success">Active</Badge>
          </div>

          <div class="overflow-x-auto my-2">
            <table class="table table-sm">
              <tbody>
                <tr class="border-b border-base-content/10">
                  <td class="text-base-content/70">Fabric Commissioning</td>
                  <td class="text-right">
                    <Badge variant={isPaired() ? 'success' : 'warning'}>
                      {isPaired() ? 'Paired' : 'Unpaired'}
                    </Badge>
                  </td>
                </tr>
                <tr class="border-b border-base-content/10">
                  <td class="text-base-content/70">🔒 Door Lock (EP1)</td>
                  <td class="text-right">{renderSubBadge(propsObj().door_lock_subscribed)}</td>
                </tr>
                <tr class="border-b border-base-content/10">
                  <td class="text-base-content/70">🔔 Ring / Occupancy (EP2)</td>
                  <td class="text-right">{renderSubBadge(propsObj().ring_sensor_subscribed)}</td>
                </tr>
              </tbody>
            </table>
          </div>

          <p class="text-xs text-base-content/60 mt-2">
            Readiness barrier ensures Apple HomeKit controllers prime cluster subscriptions before marking the bridge Operational.
          </p>
        </CardBody>
      </Card>

      {/* Smart Home Integration Info */}
      <Card bordered>
        <CardBody>
          <CardTitle class="text-base font-bold text-base-content mb-2">
            Ecosystem Integrations
          </CardTitle>
          <p class="text-sm text-base-content/80 mb-3">
            Intercom Bridge bridges analog intercom hardware to major smart home ecosystems via Matter.
          </p>
          <ul class="space-y-2 text-sm text-base-content/70">
            <li class="flex items-center gap-2">
              <Check class="w-4 h-4 text-success" />
              <span>Apple Home (HomeKit door lock + doorbell chime)</span>
            </li>
            <li class="flex items-center gap-2">
              <Check class="w-4 h-4 text-success" />
              <span>Home Assistant Matter integration</span>
            </li>
            <li class="flex items-center gap-2">
              <Check class="w-4 h-4 text-success" />
              <span>Google Home & Amazon Alexa compatible</span>
            </li>
          </ul>
        </CardBody>
      </Card>
    </div>
  );
};
```

- [ ] **Step 3: Write `src/components/DiagnosticsTab.tsx`**

`crates/intercom-adapter-http/frontend/src/components/DiagnosticsTab.tsx`:
```tsx
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
    const hrs = Math.floor(sec / 3600);
    const mins = Math.floor((sec % 3600) / 60);
    const secs = sec % 60;
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
                  <td class="text-right font-mono">{props.status?.wifi_ssid || 'Disconnected'}</td>
                </tr>
                <tr class="border-b border-base-content/10">
                  <td class="text-base-content/70">Wi-Fi RSSI</td>
                  <td class="text-right font-mono">
                    {props.status?.wifi_rssi_dbm != null ? `${props.status.wifi_rssi_dbm} dBm` : 'N/A'}
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
        </CardBody>
      </Card>

      {/* Memory & Partitions */}
      <Card bordered>
        <CardBody>
          <CardTitle>Memory & Firmware</CardTitle>

          <div class="my-2">
            <div class="flex justify-between text-xs mb-1">
              <span class="text-base-content/70">Heap Utilization</span>
              <span class="font-mono">{heapInfo().usedKb} KB / {heapInfo().totalKb} KB ({heapInfo().pct}%)</span>
            </div>
            <progress class="progress progress-primary w-full" value={heapInfo().pct} max="100" />
          </div>

          <div class="overflow-x-auto mt-2">
            <table class="table table-sm">
              <tbody>
                <tr class="border-b border-base-content/10">
                  <td class="text-base-content/70">Active Partition</td>
                  <td class="text-right font-mono">
                    <Badge variant="primary">{props.status?.active_slot || 'ota_0'}</Badge>
                  </td>
                </tr>
                <tr class="border-b border-base-content/10">
                  <td class="text-base-content/70">Slot Boot Valid</td>
                  <td class="text-right">
                    <Badge variant={props.status?.slot_valid ? 'success' : 'warning'}>
                      {props.status?.slot_valid ? 'Valid' : 'Pending'}
                    </Badge>
                  </td>
                </tr>
                <tr class="border-b border-base-content/10">
                  <td class="text-base-content/70">Firmware Version</td>
                  <td class="text-right font-mono">{props.status?.version || '0.1.0'}</td>
                </tr>
                <tr class="border-b border-base-content/10">
                  <td class="text-base-content/70">Git Commit</td>
                  <td class="text-right font-mono">{props.status?.git_commit || 'dev'}</td>
                </tr>
                <tr class="border-b border-base-content/10">
                  <td class="text-base-content/70">Build Date</td>
                  <td class="text-right font-mono">{props.status?.build_date || '--'}</td>
                </tr>
              </tbody>
            </table>
          </div>
        </CardBody>
      </Card>
    </div>
  );
};
```

- [ ] **Step 4: Write `src/components/LogsTab.tsx`**

`crates/intercom-adapter-http/frontend/src/components/LogsTab.tsx`:
```tsx
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
    if (isPaused()) return;
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
```

- [ ] **Step 5: Write `src/App.tsx`, `src/main.tsx`, and `index.html`**

`crates/intercom-adapter-http/frontend/src/App.tsx`:
```tsx
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

  const tabs: { id: TabId; label: string; icon: Component<{ class?: string }> }[] = [
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
```

`crates/intercom-adapter-http/frontend/src/main.tsx`:
```tsx
import { render } from 'solid-js/web';
import { App } from './App';
import './index.css';

const root = document.getElementById('root');
if (root) {
  render(() => <App />, root);
}
```

`crates/intercom-adapter-http/frontend/index.html`:
```html
<!DOCTYPE html>
<html lang="en" data-theme="dark">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Intercom Bridge</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

- [ ] **Step 6: Verify with Biome check**

Run: `cd crates/intercom-adapter-http/frontend && mise exec -- bun run check`
Expected: Biome check and formatting pass.

- [ ] **Step 7: Commit**

```bash
git add crates/intercom-adapter-http/frontend
git commit -m "feat(frontend): implement feature tabs (Controls, Ecosystem, Diagnostics, Logs) and App shell"
```

---

### Task 5: Frontend Build & Single-File Asset Generation

**Files:**
- Create: `crates/intercom-adapter-http/assets/dashboard.html` (generated by build)

**Interfaces:**
- Produces: `crates/intercom-adapter-http/assets/dashboard.html` containing complete inlined CSS, JS, and SVG.

- [ ] **Step 1: Execute singlefile bundle build**

Run: `cd crates/intercom-adapter-http/frontend && mise exec -- bun run build`
Expected: Outputs `crates/intercom-adapter-http/assets/dashboard.html`.

- [ ] **Step 2: Verify generated bundle size and content**

Run: `wc -c crates/intercom-adapter-http/assets/dashboard.html`
Expected: File exists and contains `<!DOCTYPE html>` and `Intercom Bridge`.

- [ ] **Step 3: Commit**

```bash
git add crates/intercom-adapter-http/assets/dashboard.html
git commit -m "feat(frontend): build and commit self-contained dashboard.html bundle"
```

---

### Task 6: Update Rust `build.rs`, `lib.rs` & Unit Tests

**Files:**
- Modify: `crates/intercom-adapter-http/build.rs`
- Modify: `crates/intercom-adapter-http/src/lib.rs`
- Remove: `crates/intercom-adapter-http/src/assets/dashboard.html`

**Interfaces:**
- Consumes: `crates/intercom-adapter-http/assets/dashboard.html`
- Produces: `DASHBOARD_HTML_GZ` baked into firmware binary with passing unit tests.

- [ ] **Step 1: Update `build.rs` to watch and read `assets/dashboard.html`**

`crates/intercom-adapter-http/build.rs`:
```rust
use std::env;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use flate2::write::GzEncoder;
use flate2::Compression;

fn main() {
    println!("cargo:rerun-if-changed=assets/dashboard.html");

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR environment variable not set");
    let dest_path = PathBuf::from(out_dir).join("dashboard.html.gz");

    let html_bytes = fs::read("assets/dashboard.html")
        .expect("Failed to read assets/dashboard.html");

    let output_file = File::create(&dest_path)
        .expect("Failed to create destination dashboard.html.gz");
    let mut encoder = GzEncoder::new(output_file, Compression::best());
    encoder
        .write_all(&html_bytes)
        .expect("Failed to compress dashboard.html");
    encoder
        .finish()
        .expect("Failed to finish gzip compression");
}
```

- [ ] **Step 2: Remove old `src/assets/dashboard.html` and update `lib.rs` tests**

Update `crates/intercom-adapter-http/src/lib.rs` test to assert "Intercom Bridge":

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use flate2::read::GzDecoder;
    use std::io::Read;

    #[test]
    fn test_dashboard_gzip_decompresses_to_valid_html() {
        // Arrange
        let mut decoder = GzDecoder::new(DASHBOARD_HTML_GZ);
        let mut html = std::string::String::new();

        // Act
        decoder.read_to_string(&mut html).expect("Decompression failed");

        // Assert
        assert!(html.to_lowercase().contains("<!doctype html>"));
        assert!(html.contains("Intercom Bridge"));
        assert!(
            DASHBOARD_HTML_GZ.len() < 50 * 1024,
            "Gzip asset exceeds 50 KiB limit: {} bytes",
            DASHBOARD_HTML_GZ.len()
        );
    }
}
```

Delete: `rm -rf crates/intercom-adapter-http/src/assets`

- [ ] **Step 3: Run `intercom-adapter-http` unit tests**

Run: `cargo test -p intercom-adapter-http --target x86_64-apple-darwin`
Expected: All tests pass including `test_dashboard_gzip_decompresses_to_valid_html`.

- [ ] **Step 4: Commit**

```bash
git add crates/intercom-adapter-http/build.rs crates/intercom-adapter-http/src/lib.rs
git rm -r crates/intercom-adapter-http/src/assets
git commit -m "feat(http): point build.rs to assets/dashboard.html and remove legacy html"
```

---

### Task 7: Full Firmware Build Verification

**Files:**
- Test: Host workspace test suite
- Test: RISC-V firmware build (`intercom-firmware`)

- [ ] **Step 1: Run host unit tests across workspace**

Run: `cargo test -p intercom-core -p intercom-adapter-http -p intercom-adapter-ota --target x86_64-apple-darwin`
Expected: All tests pass.

- [ ] **Step 2: Build release firmware binary**

Run: `mise exec -- cargo build -p intercom-firmware --release`
Expected: Compiles with 0 errors and generates release ELF with embedded gzip dashboard.

- [ ] **Step 3: Commit any final cleanup or updates to AGENTS.md / docs**

```bash
git add AGENTS.md
git commit -m "docs: document modern Solid.js frontend workflow in AGENTS.md"
```
