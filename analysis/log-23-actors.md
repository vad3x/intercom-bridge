# Log-23 Network Actors

Analysis of `log-23-merged.log` — C++ ESP-IDF Matter device commissioning session.

## Log Sources

| Prefix | Source | Lines |
|--------|--------|-------|
| `[DEV]` | ESP32-C6 serial output — **C++ ESP-IDF** Matter device (legacy codebase) | 726 |
| `[PHN]` | iPhone HomeKitDaemon syslog | 5,318 |
| `[TCP]` | tcpdump packet capture (WiFi) | 278 |

> **Note:** Unlike log-22 (Rust `rs-matter` device), log-23's `[DEV]` is the **legacy C++ ESP-IDF** Matter device (`chip[SVR]`, `chip[DL]`, `MatterDriver` log tags).

## Primary Actors

| # | Device | MAC | IPv4 | IPv6 (link-local) | Role |
|---|--------|-----|------|-----|------|
| 1 | **ESP32-C6 C++ Device** (Smart Intercom) | `98:a3:16:a6:78:44` | `.120` | `fe80::9aa3:16ff:fea6:7844` | Matter device under test (port 5540), hostname `98A316A67844.local`, `[DEV]` log source. Commissioned via PASE→BLE during this log. |
| 2 | **Vadim's iPhone** (16 Pro Max) | `5e:88:cd:45:6d:7d` | `.124` | `fe80::1c6a:fa48:149c:eb73` | Matter commissioner, `Vadim-iPhone.local`, `[PHN]` log source |
| 3 | **Vadim's iMac** (M4) | `4e:48:31:b3:98:f4` | `.137` | `fe80::420:519d:e431:3a18` | `G6HPX2M46Q.local`, model `Mac15,7`, AirPlay/RAOP/Companion-Link. Sends CASE Sigma1 to the device over IPv6:5540. |
| 4 | **Aqara Hub M3** (Thread Border Router) | `54:ef:44:80:59:60` | `.111` | `fe80::56ef:44ff:fe80:5960` | Sends IPv6 RA, Matter fabric `048BDC4364465DE2`, `Aqara-Hub-M3-5960` |
| 5 | **Vadim's Living Room** (HomePod) | `a8:51:ab:2d:15:c1` | `.115` | `fe80::cac:fea7:8682:4d60` | HomeKit hub, model `J255AP` (HomePod), queries `_sleep-proxy._udp`, `_meshcop._udp` |
| 6 | **WiFi Access Point** | `3a:60:f9:de:37:40` | gateway `.1` | — | BSSID for SSID `vs_us`, channel 1, BW20, WiFi 6 (HE) |

## Apple HomeKit Hubs (Matter Controllers)

All advertise fabrics `47F49EDF43015297` + `DC86FEB5BB9F8FDF` unless noted. Same IPs as log-22.

| # | MAC | IPv4 | Extra Fabrics |
|---|-----|------|---------------|
| 7 | `8c:86:dd:a6:c6:8f` | `.102` | — |
| 8 | `50:3d:d1:51:59:43` | `.103` | — |
| 9 | `8c:86:dd:a6:f8:b2` | `.104` | — |
| 10 | `50:3d:d1:e3:1d:1f` | `.105` | + `048BDC4364465DE2` (Aqara) |
| 11 | `50:3d:d1:51:5b:a4` | `.106` | — |
| 12 | `0c:ef:15:f5:b8:a2` | `.107` | — |
| 13 | `50:3d:d1:e3:29:1e` | `.108` | — |
| 14 | `8c:86:dd:a6:f7:5a` | `.109` | — |
| 15 | `3c:31:74:e8:d2:66` | `.113` | + `6AA61A6C3A93F404` |

## Other Apple Devices

| # | MAC | IPv4 | IPv6 (link-local) | Notes |
|---|-----|------|-----|-------|
| 16 | `b6:b2:d4:ea:62:90` | `.151` | `fe80::c7:ed26:58ac:47d1` | Apple device (randomized MAC). Queries HAP, Matter, AirPlay, Companion-Link. Receives CASE StatusReport from C++ device. |
| 17 | `46:98:b0:6e:92:05` | `.173` | — | Apple device (randomized MAC). iMac queries it for AirPlay/RAOP. |
| 18 | `0a:86:03:9c:8d:98` | `.139` | `fe80::1439:2256:1906:626e` | Advertises RAOP for "Vadim's iMac (145)" — likely iMac secondary interface or wired Ethernet. |

## HomeKit Accessory Identifiers (from `[PHN]` logs)

These are **HomeKit logical identifiers** (not network MACs) — Matter devices known to the iPhone:

| HK Identifier | Name / Model | Fabric | Node ID | Category | Status |
|---------------|-------------|--------|---------|----------|--------|
| `5b:26:ff:b8:72:90` | Smart Intercom | `FD99106B3D86A0DE` | `60C721CD` | — | **Being commissioned in this log** (the C++ device at `.120`) |
| `f4:a9:a5:62:29:43` | Smart Intercom | `FD99106B3D86A0DE` | `97915AD8` | 6 (Door Lock) | Previously commissioned, WiFi:YES |
| `c0:60:37:9a:01:2b` | ACME Test | `FD99106B3D86A0DE` | `A043AB3A` | 5 | Previously commissioned, WiFi:YES |

## Matter Fabrics Summary

| Fabric ID | Owner | Nodes in this log |
|-----------|-------|-------------------|
| `47F49EDF43015297` | Apple Home #1 | All 9 Apple hubs; iPhone subscribes to node `77FB03C8` at `.101:5540` |
| `DC86FEB5BB9F8FDF` | Apple Home #2 | All 9 Apple hubs + **new: C++ device** gets node `BDBE23FA` during commissioning |
| `048BDC4364465DE2` | Aqara Home | Aqara Hub M3 (`.111`) + hub at `.105` |
| `6AA61A6C3A93F404` | Unknown fabric | Hub at `.113` only |
| `FD99106B3D86A0DE` | Development fabric | C++ device (`60C721CD`), previously commissioned Smart Intercom (`97915AD8`), ACME Test (`A043AB3A`) |

## Commissioning Flow (key timeline)

| Time | Event |
|------|-------|
| `16:36:02` | ESP32-C6 boots (C++ ESP-IDF firmware) |
| `16:36:10` | Commissioning window opened, advertising `VID=65521 PID=32769 disc=3840` |
| `16:37:29` | PASE session establishment starts (over BLE) |
| `16:37:30` | PASE Pake1→Pake2→Pake3 complete, commissioning window closed |
| `16:37:34` | AddNOC received — operational credentials installed |
| `16:37:36` | WiFi connected to `vs_us` (BSSID `3a:60:f9:de:37:40`) |
| `16:37:38` | Got IPv4 `192.168.0.120`, starts advertising on `FD99106B3D86A0DE` fabric |
| `16:37:44` | Assigned to Apple Home fabric `DC86FEB5BB9F8FDF`, node `BDBE23FA` |
| `16:37:46` | Advertising both fabrics on mDNS |
| `16:37:51` | iMac sends first CASE Sigma1 to device at `fe80::9aa3:...:5540` |
