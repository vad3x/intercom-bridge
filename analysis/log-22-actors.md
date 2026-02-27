# Log-22 Network Actors

Analysis of `log-22-merged.log` — Rust `rs-matter` intercom device commissioning session.

## Log Sources

| Prefix | Source | Lines |
|--------|--------|-------|
| `[DEV]` | ESP32-C6 serial output (Rust rs-matter intercom) | 450 |
| `[PHN]` | iPhone HomeKitDaemon syslog | 2,752 |
| `[TCP]` | tcpdump packet capture (WiFi) | 228 |

## Primary Actors

| # | Device | MAC | IPv4 | IPv6 (link-local) | Role |
|---|--------|-----|------|-----|------|
| 1 | **ESP32-C6 Intercom** | `9c:13:9e:cb:ce:08` | `.123` | `fe80::9e13:9eff:fecb:ce08` | Matter device under test (port 5540), hostname `9C139ECBCE080000.local`, fabric `FD99106B3D86A0DE` |
| 2 | **Vadim's iPhone** (16 Pro Max) | `5e:88:cd:45:6d:7d` | `.124` | `fe80::1c6a:fa48:149c:eb73` | Matter commissioner, `Vadim-iPhone.local`, model `D94AP`, `[PHN]` log source |
| 3 | **Vadim's iMac** (M4) | `4e:48:31:b3:98:f4` | `.137` | `fe80::420:519d:e431:3a18` | Observer, `G6HPX2M46Q`, model `Mac15,7`, AirPlay/RAOP/Companion-Link |
| 4 | **Aqara Hub M3** (Thread Border Router) | `54:ef:44:80:59:60` | `.111` | `fe80::56ef:44ff:fe80:5960` | Sends IPv6 Router Advertisements, Matter fabric `048BDC4364465DE2`, `Aqara-Hub-M3-5960` |
| 5 | **Vadim's Living Room** (HomePod) | `a8:51:ab:2d:15:c1` | `.115` | `fe80::cac:fea7:8682:4d60` | HomeKit hub, `Vadims-Living-Room.local`, queries Aqara Hub |

## Apple HomeKit Hubs (Matter Controllers)

All advertise fabrics `47F49EDF43015297` + `DC86FEB5BB9F8FDF` unless noted.

| # | MAC | IPv4 | IPv6 (link-local) | Extra Fabrics |
|---|-----|------|-----|---------------|
| 6 | `8c:86:dd:a6:c6:8f` | `.102` | `fe80::8e86:ddff:fea6:c68f` | — |
| 7 | `50:3d:d1:51:59:43` | `.103` | `fe80::523d:d1ff:fe51:5943` | — |
| 8 | `8c:86:dd:a6:f8:b2` | `.104` | `fe80::8e86:ddff:fea6:f8b2` | — |
| 9 | `50:3d:d1:e3:1d:1f` | `.105` | `fe80::523d:d1ff:fee3:1d1f` | + `048BDC4364465DE2` (Aqara) |
| 10 | `50:3d:d1:51:5b:a4` | `.106` | `fe80::523d:d1ff:fe51:5ba4` | — |
| 11 | `0c:ef:15:f5:b8:a2` | `.107` | `fe80::eef:15ff:fef5:b8a2` | — |
| 12 | `50:3d:d1:e3:29:1e` | `.108` | `fe80::523d:d1ff:fee3:291e` | — |
| 13 | `8c:86:dd:a6:f7:5a` | `.109` | `fe80::8e86:ddff:fea6:f75a` | — |
| 14 | `3c:31:74:e8:d2:66` | `.113` | `fe80::3e31:74ff:fee8:d266` | + `6AA61A6C3A93F404` |

## Other / Peripheral

| # | MAC | IPv4 | IPv6 (link-local) | Notes |
|---|-----|------|-----|-------|
| 15 | `66:ce:33:eb:39:b1` | `.121` | `fe80::8ba:d879:7ebc:1dc8` | Another iPhone 16 Pro Max (`D94AP`), AirPlay/RAOP/Companion-Link — likely same iPhone before MAC rotation or a 2nd iPhone |
| 16 | `b6:b2:d4:ea:62:90` | — | `fe80::c7:ed26:58ac:47d1` | Only NDP with iMac — secondary iMac interface or connected accessory |
| 17 | `98:a3:16:a6:79:3c` | — | — | Apple device (OUI `98:A3:16`), visible to ESP32 via MLD reports only |
| 18 | `b2:99:2f:d0:9b:12` | — | — | Unknown device, sends Router Solicitation (seen by ESP32 only) |

## Multicast Addresses

| Address | Purpose |
|---------|---------|
| `01:00:5e:00:00:fb` / `33:33:00:00:00:fb` | mDNS multicast (IPv4 / IPv6) |
| `33:33:00:00:00:01` | All IPv6 nodes (Router Advertisements) |
| `33:33:00:00:00:02` | All IPv6 routers (Router Solicitation) |
| `33:33:00:00:00:16` | MLDv2 reports |

## Matter Fabrics Summary

| Fabric ID | Owner | Hubs |
|-----------|-------|------|
| `47F49EDF43015297` | Apple Home #1 | All 9 Apple hubs |
| `DC86FEB5BB9F8FDF` | Apple Home #2 | All 9 Apple hubs |
| `048BDC4364465DE2` | Aqara Home | Aqara Hub M3 (`.111`) + hub at `.105` |
| `6AA61A6C3A93F404` | Unknown fabric | Hub at `.113` only |
| `FD99106B3D86A0DE` | ESP32-C6 device | The intercom itself (after commissioning) |
