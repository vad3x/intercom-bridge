# Intercom Bridge Web Dashboard (Frontend)

Modern, lightweight, responsive web dashboard built with **Solid.js**, **DaisyUI**, and **Tailwind CSS**, managed with **Bun**.

Bundles into a single, self-contained `assets/dashboard.html` (currently about 31 KB gzipped) embedded directly in the ESP32-C6 firmware binary.

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
