---
name: apple-home-matter
description: Use when integrating, commissioning, or troubleshooting Matter devices with Apple Home (HomeKit), Apple Home Hubs (HomePod, Apple TV), iOS pairing flows, or HomeKit ecosystem requirements.
---

# Apple Home Matter Integration & Troubleshooting Guide

## Overview
Apple Home natively supports Matter devices over Wi-Fi and Thread. Unlike pure local point-to-point accessories, Apple Home uses an **Apple Home Hub** (HomePod or Apple TV) as the primary Matter Controller for operational control, automation, and remote access.

---

## When to Use
- Commissioning a Matter device into Apple Home (iOS Home app).
- Troubleshooting pairing failures: `"Unable to Add Accessory"`, hanging on `"Connecting to Accessory..."`, or 37-second timeouts.
- Device shows `"Accessory Not Responding"` in the Apple Home UI.
- Dealing with uncertified accessory warning banners (`0xFFF1` test Vendor ID).
- Configuring home network hardware (Omada, UniFi, ASUS) to properly route Matter IPv6 traffic between Apple Hubs and accessories.

---

## 1. Apple Home Architecture & Requirements

### The Apple Home Hub is Mandatory
```text
[ iPhone (iOS Home App) ]
         │
         │ (1) Commissioning via BLE (PASE) & Wi-Fi Provisioning
         ▼
[ Matter Accessory (ESP32) ]
         ▲
         │ (2) Operational CASE Session over IPv6 UDP 5540
         │     (Subscriptions, Commands, Keep-Alives)
         ▼
[ Apple Home Hub (HomePod / Apple TV 4K) ]
```

- **iPhone Role:** The iPhone only acts as a temporary commissioner. It discovers the device via BLE, authenticates via PASE, and sends Wi-Fi credentials.
- **Hub Role:** Once Wi-Fi is connected, the iPhone instructs the **Apple Home Hub** (HomePod mini, HomePod, or Apple TV) to perform operational CASE discovery and take ownership of the accessory fabric.
- **Critical Implication:** If the Apple Home Hub cannot reach the device over IPv6 unicast UDP 5540, pairing **will fail**, even if the iPhone has perfect Wi-Fi connectivity.

---

## 2. Network Infrastructure: The Make-or-Break Rules

The majority of Apple Home Matter commissioning failures stem from local network router/switch settings.

### A. IPv6 Neighbor Discovery (NDP) & MLD Snooping
Apple Home Hubs communicate with accessories using **IPv6 Link-Local (`fe80::...`)** addresses:
1. The Hub sends an IPv6 **Neighbor Solicitation (NS)** multicast frame to resolve the accessory MAC address.
2. Enterprise/prosumer APs and switches (TP-Link Omada, Ubiquiti UniFi) enable **MLD Snooping** by default.
3. If the embedded device uses `smoltcp` (or emits MLDv2 without MLDv1 join reports), switches drop the multicast solicitation.
4. **Fix:**
   - In `rs-matter-embassy`, enable `mld-v1-compat` and `nd-learn-from-frames`.
   - In router/switch settings, temporarily disable MLD Snooping or ensure MLDv1 queries are enabled.

### B. Wi-Fi Multicast & Client Isolation
- **Disable "Block LAN to WLAN Multicast":** Apple Home relies on mDNS (`_matter._tcp.local`, `_matterc._udp.local`) and ICMPv6 router advertisements. Blocking multicast stops discovery completely.
- **Disable AP / Client Isolation:** The Home Hub and Matter accessory must be able to send layer-2 unicast and multicast directly to each other.
- **Same Subnet / VLAN:** Keep Apple Home Hubs and Matter accessories on the same VLAN unless an mDNS repeater/reflector (Avahi) and IPv6 routing are explicitly configured.

---

## 3. Apple Home Supported Device Types & UI Mappings

Apple Home only renders controls for standard Matter device types:

| Device Type | Matter Code | Apple Home UI Representation | HomeKit Behaviors |
|---|---|---|---|
| **Door Lock** | `0x000A` | Lock Tile (Locked / Unlocked) | One-tap unlock, chime/alerts, Siri voice control ("Hey Siri, unlock the door"). |
| **Occupancy Sensor** | `0x0107` | Motion / Occupancy Sensor | Push notifications on occupancy change, triggers home automations (e.g. lights on ring). |
| **On/Off Light** | `0x0100` | Light bulb tile | Power toggle, brightness slider (if Level Control included). |
| **Contact Sensor** | `0x0015` | Open / Closed Sensor | Instant notification on open/close. |

> [!NOTE]
> Custom or vendor-specific clusters are **not displayed** in Apple Home's native UI. Third-party iOS apps like *Controller for HomeKit* or *Eve* can inspect lower-level attributes.

---

## 4. Commissioning Diagnostics & Failure Modes

### Failure 1: The 37-Second Timeout ("Unable to Add Accessory")
* **Symptom:** BLE connects, Wi-Fi connects, iPhone shows `"Connecting to Accessory..."` for exactly 37 seconds, then aborts with `"Unable to Add Accessory"`.
* **Serial Log Signature:**
  ```text
  [DEBUG] rs_matter::transport::exchange: Too many retransmissions. Giving up
  [ERROR] rs_matter_embassy::matter: Matter runner failed: Error(Network)
  [DEBUG] esp_radio::ble: Disconnected: Remote User Terminated Connection
  ```
* **Root Cause:** Operational CASE handshake failure. The HomePod sent UDP packets, but either:
  1. IPv6 NDP Neighbor Solicitation was dropped by the Wi-Fi AP/switch (MLD snooping blackhole).
  2. The accessory could not resolve the HomePod's MAC address to send the `Sigma2` response.
* **Fix:** Enable `nd-learn-from-frames` in `rs-matter-embassy` and ensure router passes IPv6 link-local multicast.

---

### Failure 2: "Uncertified Accessory" Warning
* **Symptom:** iOS displays a modal: *"This accessory is not certified for Matter... Add Anyway?"*
* **Root Cause:** Standard behavior when using test Vendor IDs (e.g., `0xFFF1` to `0xFFF4`) without a CSA-signed Declaration of Conformity (DAC).
* **Fix:** Tap **"Add Anyway"** or **"Add to Home"**. Functionality is 100% normal.

---

### Failure 3: Device Shows "No Response" in Apple Home
* **Symptom:** Pairing succeeded, but hours or days later the tile displays *"No Response"*.
* **Root Cause:**
  1. **Wi-Fi Power Save / Sleep:** Accessory Wi-Fi radio went to sleep and missed MRP keep-alives or subscription heartbeats.
  2. **DHCP / IPv6 Renewal:** Device did not maintain its IPv6 link-local or global address.
* **Fix:** Disable Wi-Fi modem sleep (`esp_wifi_set_ps(WIFI_PS_NONE)`) or tune DTIM listening intervals in embedded firmware.

---

### Failure 4: Device Fails to Re-Pair ("Already Added")
* **Symptom:** Flashing new firmware causes pairing to fail immediately or hang on BLE.
* **Root Cause:** Existing fabric credentials remain stored in flash partition (`matter_kvs` or `nvs`).
* **Fix:** Erase Matter non-volatile storage:
  ```bash
  espflash erase-parts --port <PORT> matter_kvs nvs
  ```

---

### Failure 5: Device Not Returning to Pairing Mode After Removing From Apple Home
* **Symptom:** User taps "Remove Accessory" in the Apple Home app, but the accessory never re-enters BLE pairing mode and remains considered commissioned.
* **Root Cause (Apple Home 2-Fabric Model):**
  - Apple Home creates two separate fabrics:
    1. Primary Controller Fabric: `vendor_id = 4937` (`0x1349`, Apple Inc. iOS / iPadOS Commissioner).
    2. Operational Home Hub Fabric: `vendor_id = 4996` (`0x1384`, Apple Inc. Home Hub — HomePod / Apple TV).
  - When removing an accessory from Apple Home, iOS only sends `RemoveFabric` for its primary commissioner fabric (`4937`), leaving the Home Hub fabric (`4996`) orphaned in flash.
  - Naive firmware checks like `matter.has_fabrics()` continue to return `true` because the orphaned hub fabric is still present in storage.
* **Fix (Pure Software Auto-Reset):**
  - Define `APPLE_HOME_HUB_VENDOR_ID = 4996` (`0x1384`).
  - Check for active controller fabrics:
    ```rust
    fn has_active_controller_fabric(matter: &rs_matter::Matter<'_>) -> bool {
        matter.with_state(|state| {
            state.fabrics.iter().any(|f| f.vendor_id() != APPLE_HOME_HUB_VENDOR_ID)
        })
    }
    ```
  - **On boot:** If only orphaned fabric `4996` exists (`has_fabrics() && !has_active_controller_fabric()`), wipe `matter_kvs` and reboot.
  - **At runtime:** When transitioning from commissioned to having no controller fabrics, trigger `on_decommissioned()`, wipe `matter_kvs`, and reset into pairing mode.

---

## 5. Serial Debugging Without Rebooting

Standard terminal monitors toggle DTR/RTS which reboots ESP32 chips and destroys in-memory state:

```bash
# Monitor live logs NON-INTRUSIVELY (does NOT reset running chip):
python3 scripts/monitor.py --no-reset

# Follow persistent output log:
tail -f /tmp/intercom_serial.log
```
