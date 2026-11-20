# Intercom Bridge — Home Assistant Custom Integration & Lovelace Card

Provides a **Listen**, **Hold-to-Talk**, and **Matter Unlock** Lovelace card (`custom:intercom-bridge-card`) that works inside Home Assistant's `https://` UI and Companion App without mixed-content errors or TLS certificates on the ESP32.

## How It Works
- Proxies `/api/ws/audio` over Home Assistant's authenticated `wss://` endpoint (`/api/intercom_bridge/ws/<entry_id>?token=...`) so browser microphone capture (`getUserMedia`) works cleanly.
- Automatically registers `/intercom_bridge/intercom-bridge-card.js` as a Lovelace frontend resource.
- Uses your existing Matter `lock.*` entity for door unlock and `event.*` / `binary_sensor.*` entity for ring state.

## Installation
1. Copy `custom_components/intercom_bridge` into your Home Assistant `<config>/custom_components/intercom_bridge` directory.
2. Restart Home Assistant.
3. Go to **Settings → Devices & Services → Add Integration → Intercom Bridge**, and enter your ESP32's IP/hostname (e.g. `192.168.0.180`, port `8080`).
4. Add the card to any Lovelace dashboard:

```yaml
type: custom:intercom-bridge-card
title: Apartment Intercom
lock_entity: lock.intercom_bridge_door_lock
ring_entity: event.intercom_bridge_doorbell
```

> **Note:** Microphone access requires accessing Home Assistant over `https://` (or `http://localhost`).
