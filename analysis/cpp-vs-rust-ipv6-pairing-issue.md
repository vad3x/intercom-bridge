# Analysis Prompt: Why Does the Rust Device Fail CASE While the C++ Device Succeeds?

## Context

I'm commissioning two ESP32-C6 Matter devices on the same WiFi network. Both go through BLE commissioning (PASE) successfully, then the iPhone tries to establish a CASE session over WiFi (UDP port 5540). The C++ device succeeds; the Rust device fails.

The two log files to analyze:

- `log-22-merged.log` — **Rust device (rs-matter-embassy), FAILS** at the CASE step
- `log-23-merged.log` — **C++ device (ESP-IDF + connectedhomeip), SUCCEEDS**

Both logs are merged from three sources:

- `[DEV]` = device serial output
- `[PHN]` = iPhone sysdiagnose logs (Matter framework)
- `[TCP]` = tcpdump on MacBook (`tcpdump -i en0 -n -e 'icmp6 or port 5353 or port 5540'`)

Same WiFi network (SSID: `vs_us`, WPA3-SAE), same iPhone, same Apple Home setup.

---

## Network Actors (same in both sessions)

| Role | MAC | IPv4 | IPv6 link-local | IPv6 ULA |
|------|-----|------|----------------|----------|
| **Rust Device** | `9c:13:9e:cb:ce:08` | `192.168.0.123` | `fe80::9e13:9eff:fecb:ce08` | ❌ never acquired |
| **C++ Device** | `98:a3:16:a6:78:44` | `192.168.0.120` | `fe80::9aa3:16ff:fea6:7844` | `fd23:d9f7:d895:42b7:9aa3:16ff:fea6:7844` |
| **iPhone** | `5e:88:cd:45:6d:7d` | `192.168.0.124` | `fe80::1c6a:fa48:149c:eb73` | `fd23:d9f7:d895:42b7:1884:4d1:cca3:ae28` |
| **Home Hub (Apple TV?)** | `4e:48:31:b3:98:f4` | `192.168.0.137` | `fe80::420:519d:e431:3a18` | — |
| **Router** | `54:ef:44:80:59:60` | `192.168.0.111` | `fe80::56ef:44ff:fe80:5960` | — |

---

## Side-by-Side Commissioning Flow Comparison

Below is a step-by-step comparison of every commissioning stage. **Identical steps are collapsed; differences are highlighted.**

### Phase 1: BLE PASE Pairing — ✅ Both Succeed

| Step | C++ Device (log-23) | Rust Device (log-22) |
|------|---------------------|----------------------|
| `SecurePairing` → `ReadCommissioningInfo` | ✅ 16:37:30.533 | ✅ 16:28:30.815 |
| `ReadCommissioningInfo` → `ArmFailSafe` | ✅ 16:37:30.876 | ✅ 16:28:31.150 |
| `ArmFailSafe` → `ConfigRegulatory` | ✅ 16:37:30.995 | ✅ 16:28:31.269 |
| `ConfigRegulatory` → `ConfigureTCAcknowledgments` | ✅ 16:37:31.086 | ✅ 16:28:31.391 |
| `ConfigureTCAcknowledgments` → `SendPAICertificateRequest` | ✅ 16:37:31.087 | ✅ 16:28:31.391 |
| `SendPAICertificateRequest` → `SendDACCertificateRequest` | ✅ 16:37:31.358 | ✅ 16:28:31.719 |
| `SendDACCertificateRequest` → `SendAttestationRequest` | ✅ 16:37:31.575 | ✅ 16:28:31.961 |
| `SendAttestationRequest` → `AttestationVerification` | ✅ 16:37:31.897 | ✅ 16:28:32.830 |
| `AttestationVerification` | ⚠️ Error 0x000000AC (non-fatal, continues) | ⚠️ Same error 0x000000AC (non-fatal) |
| `AttestationRevocationCheck` → `SendOpCertSigningRequest` | ✅ 16:37:33.358 | ✅ 16:28:34.647 |
| `SendOpCertSigningRequest` → `ValidateCSR` | ✅ 16:37:33.877 | ✅ 16:28:35.983 |
| `ValidateCSR` → `GenerateNOCChain` | ✅ 16:37:33.878 | ✅ 16:28:35.984 |
| `GenerateNOCChain` → `SendTrustedRootCert` | ✅ 16:37:33.879 | ✅ 16:28:35.985 |
| `SendTrustedRootCert` → `SendNOC` | ✅ 16:37:34.195 | ✅ 16:28:36.220 |
| `SendNOC` → `WiFiNetworkSetup` | ✅ 16:37:34.747 | ✅ 16:28:37.392 |
| `WiFiNetworkSetup` → `FailsafeBeforeWiFiEnable` | ✅ 16:37:34.839 | ✅ 16:28:39.220 |
| `FailsafeBeforeWiFiEnable` → `WiFiNetworkEnable` | ✅ 16:37:34.926 | ✅ 16:28:39.309 |
| `WiFiNetworkEnable` completed | ✅ 16:37:36.548 | ✅ 16:28:39.547 |
| `kEvictPreviousCaseSessions` completed | ✅ 16:37:36.548 | ✅ 16:28:39.548 |

**All BLE-phase commissioning steps are IDENTICAL.** Both devices receive credentials and join WiFi successfully.

### Phase 2: WiFi Connect + IP Acquisition — ⚠️ DIVERGES HERE

| Event | C++ Device (log-23) | Rust Device (log-22) |
|-------|---------------------|----------------------|
| WiFi connected | 16:37:36.514 (`wifi:connected with vs_us`) | 16:28:39.297 (`Wifi connected`) |
| **WiFi state machine** | **Full ESP-IDF logs**: auth→assoc→run, ADDBA TX/RX, PM start, detailed PHY negotiation (HE caps, MCS, NSS, etc.) | **Minimal**: just `Wifi connected` |
| IPv4 acquired | 16:37:38.017 (`IP_EVENT_STA_GOT_IP: 192.168.0.120`) — **1.5s after connect** | 16:28:40.939 (inside Netif change) — **1.6s after connect** |
| IPv6 link-local acquired | 16:37:38.122 (`IP_EVENT_GOT_IP6: fe80::...7844`) — **separate event, 0.1s after IPv4** | 16:28:40.939 (same Netif change, simultaneous with IPv4) |
| **IPv6 ULA acquired** | **16:37:40.166** (`IP_EVENT_GOT_IP6: fd23:d9f7:d895:42b7:...`) — **2s after link-local, from Router Advertisement** | **❌ NEVER** — only link-local, no ULA |
| `Advertise operational` calls | **7 times** (16:37:34.7, 36.6, 38.0, 38.1, 40.2, 41.8) — re-advertises each time an address changes | **Only via mDNS** at 16:28:40.941 (`Running mDNS`) — single advertisement burst |
| mDNS AAAA records | `fe80::...7844` initially, then adds `fd23:d9f7:d895:...7844` (both link-local AND ULA) | `fe80::9e13:9eff:fecb:ce08` only (link-local only, no ULA) |
| **Router Advertisement processing** | RA received → ULA prefix extracted → new IPv6 address generated → IP_EVENT_GOT_IP6 fired → mDNS re-advertised | RA received (logged) but **NO new address derived from it** |

**Key difference #1**: The C++ device's ESP-IDF/lwIP stack processes Router Advertisements and derives a ULA address. The Rust device receives RA (line 2891: `ICMPv6 type=134 (RouterAdvertisement)`) but **never generates a ULA address**.

**Key difference #2**: The C++ device has a full lwIP network stack with built-in IPv6 Neighbor Discovery (NS/NA handling), DAD, and SLAAC. The Rust device uses a custom/minimal network stack in `rs-matter-embassy`.

### Phase 3: mDNS → CASE — 💥 FAILURE POINT

| Event | C++ Device (log-23) | Rust Device (log-22) |
|-------|---------------------|----------------------|
| mDNS resolution by iPhone | 16:37:39.258 → `fe80::...7844` score 7 | 16:28:43.804 → `fe80::...ce08` score 7 |
| iPhone sends Sigma1 | 16:37:39.260 → `[fe80::...7844%en0]:5540` | 16:28:43.807 → `[fe80::...ce08%en0]:5540` |
| **In tcpdump: UDP on port 5540?** | Not captured (iPhone↔Device is unicast, not visible from MacBook) | **NOT captured** (same reason? or packet never sent?) |
| **Device receives Sigma1?** | **YES**: `chip[EM]: >>> Msg RX ... CASE_Sigma1 (B:196)` at 16:37:39.758 | **NO**: no RX log whatsoever |
| Device sends Sigma2 | YES: 16:37:40.141 | N/A |
| CASE completes | YES → `CommissioningComplete` at 16:37:41.738 | N/A |
| Outcome | `kFindOperationalForStayActive` → Success → `SendComplete` → `Cleanup` ✅ | `kFindOperationalForStayActive` → 33s timeout → **Error 0x00000074: cancelled** → `Cleanup` ❌ |

### Phase 4: Post-Commission (C++ only) — Evidence of Working ND

10 seconds after commissioning completes, the home hub contacts the C++ device:

```
4956:[TCP] 4e:48:31:b3:98:f4 > 33:33:ff:a6:78:44: fe80::420:519d:e431:3a18 > ff02::1:ffa6:7844: ICMP6, neighbor solicitation, who has fe80::9aa3:16ff:fea6:7844
4957:[TCP] 98:a3:16:a6:78:44 > 4e:48:31:b3:98:f4: fe80::9aa3:16ff:fea6:7844 > fe80::420:519d:e431:3a18: ICMP6, neighbor advertisement
4958:[TCP] 4e:48:31:b3:98:f4 > 98:a3:16:a6:78:44: fe80::420:519d:e431:3a18.55630 > fe80::9aa3:16ff:fea6:7844.5540: UDP, length 196
```

This proves: **NS → NA → UDP:5540 is the required sequence**. The C++ device's lwIP stack handles NS/NA automatically. The Rust device never demonstrates this capability.

---

## Critical Observations

### 1. The Rust device only receives MULTICAST IPv6, never UNICAST

After WiFi connect, the Rust device logs these IPv6 RX entries:

```
2923:[DEV] IPv6 RX: 86 bytes, ICMPv6 type=131 (MLDv1 Report) dst=33:33:ff:a6:79:3c   ← multicast
2924:[DEV] IPv6 RX: 86 bytes, ICMPv6 type=131 (MLDv1 Report) dst=33:33:00:00:00:02     ← multicast
2925:[DEV] IPv6 RX: 86 bytes, ICMPv6 type=131 (MLDv1 Report) dst=33:33:00:00:00:fb     ← multicast
2926:[DEV] IPv6 RX: 132 bytes, proto=UDP dst=33:33:00:00:00:fb ports=57757→5353         ← multicast mDNS
2927:[DEV] IPv6 RX: 132 bytes, proto=UDP dst=33:33:00:00:00:fb ports=5353→5353          ← multicast mDNS
```

All received packets are multicast (dst MAC `33:33:*`). No unicast IPv6 packet (like a Neighbor Solicitation or UDP to port 5540) is ever received.

### 2. No ICMPv6 Neighbor Solicitation received by the Rust device

For the iPhone to send a unicast IPv6 UDP packet to `fe80::9e13:9eff:fecb:ce08`, it must first resolve the IPv6 → MAC via Neighbor Discovery (NS/NA). The Rust device never logs receiving a Neighbor Solicitation (ICMPv6 type=135). This means either:

- The iPhone never sent a NS (unlikely if it's trying to send UDP)
- The NS was sent to the solicited-node multicast `ff02::1:ffcb:ce08` (MAC `33:33:ff:cb:ce:08`) but the Rust device's WiFi stack didn't receive it
- The device received it but didn't process/log it

### 3. The C++ device DOES receive and respond to NS

In log-23, the home hub successfully does NS→NA→UDP:5540:

```
4956:[TCP] fe80::420:519d:e431:3a18 > ff02::1:ffa6:7844: ICMP6, neighbor solicitation, who has fe80::9aa3:16ff:fea6:7844
4957:[TCP] fe80::9aa3:16ff:fea6:7844 > fe80::420:519d:e431:3a18: ICMP6, neighbor advertisement
4958:[TCP] fe80::420:519d:e431:3a18.55630 > fe80::9aa3:16ff:fea6:7844.5540: UDP, length 196
```

The C++ device responds to NS because it uses ESP-IDF's lwIP stack which handles ICMPv6 ND natively.

### 4. The Rust device does NOT process Router Advertisements

The C++ device receives a Router Advertisement and derives a ULA address:

```
2213:[DEV] IP_EVENT_GOT_IP6
2214:[DEV] IPv6 addr available: fe80:0000:0000:0000:9aa3:16ff:fea6:7844  ← link-local
...
2261:[DEV] IP_EVENT_GOT_IP6
2262:[DEV] IPv6 addr available: fd23:d9f7:d895:42b7:9aa3:16ff:fea6:7844  ← ULA from RA
```

The Rust device receives the RA but does nothing with it:

```
2891:[DEV] IPv6 RX: 118 bytes, ICMPv6 type=134 (RouterAdvertisement)
(... no new address derived, no log of prefix processing ...)
```

### 5. MLDv2 group membership may be incomplete

The Rust device sends MLDv2 Reports (ICMPv6 type=143) to join multicast groups. But it's unclear whether it joins the solicited-node multicast group `ff02::1:ffcb:ce08` which is essential for receiving Neighbor Solicitations for its own address.

### 6. Both devices send mDNS successfully

Both devices' mDNS packets are visible in tcpdump — proving they CAN send IPv4 and IPv6 multicast/broadcast. The issue is specifically with **receiving unicast IPv6**.

### 7. The Rust device restarts its Matter stack 3 times before commissioning

```
263:[DEV] [16:27:48.853] INFO - Starting...
305:[DEV] [16:27:49.073] INFO - Netif down
400:[DEV] [16:27:55.702] INFO - Starting...
442:[DEV] [16:27:55.920] INFO - Netif down
586:[DEV] [16:28:01.344] INFO - Netif down
```

The stack starts and shuts down 3 times (every ~6-7 seconds) before BLE pairing begins. This might indicate a restart loop or an expected BLE advertising cycle, but is worth investigating for side effects.

---

## Things the C++ Device's Stack Does That the Rust Stack Might Not

Based on the log comparison, the C++ ESP-IDF/lwIP stack does these things that are **absent or unconfirmed** in the Rust `rs-matter-embassy` stack:

1. **ICMPv6 Neighbor Discovery (NS → NA)** — lwIP responds to Neighbor Solicitations automatically. The Rust stack shows NO evidence of ND handling.

2. **IPv6 SLAAC from Router Advertisements** — lwIP processes RA prefixes and derives additional IPv6 addresses (ULA). The Rust stack receives RA but doesn't act on it.

3. **Duplicate Address Detection (DAD)** — lwIP performs DAD before using a link-local address. The Rust stack likely skips this.

4. **Solicited-node multicast group join** — When lwIP configures an IPv6 address, it automatically joins `ff02::1:ffXX:XXXX` (solicited-node multicast for ND). The Rust stack's MLDv2 Reports may not include this group.

5. **Multiple IP event notifications** — lwIP fires separate events (`IP_EVENT_STA_GOT_IP`, `IP_EVENT_GOT_IP6`) which trigger mDNS re-advertisement. The Rust stack does a single atomic Netif state change.

6. **WiFi ADDBA negotiation logging** — C++ device shows block-ack setup. Not relevant but shows the maturity difference.

---

## Hypotheses to Investigate (Ranked by Likelihood)

1. **🔴 Missing ICMPv6 Neighbor Discovery handling** — The Rust stack likely does NOT respond to Neighbor Solicitations. Without NS→NA, the iPhone cannot resolve the device's IPv6 to its MAC address, so UDP packets can never be sent. This is the most likely root cause.

2. **🔴 Missing solicited-node multicast group join** — Even if ND is implemented, the device must join `ff02::1:ffcb:ce08` to receive the NS in the first place. The MLDv2 Report the device sends may not include this group, so the AP won't forward the NS.

3. **🟡 WiFi multicast filter issue** — The ESP32-C6 WiFi driver may not be configured to receive the solicited-node multicast group's Ethernet frames (`33:33:ff:cb:ce:08`). The device receives `33:33:00:00:00:fb` (mDNS) and `33:33:00:00:00:01` (all-nodes) but may filter out other multicast MACs.

4. **🟡 No RA processing → No ULA → reduced reachability** — While the CASE failure happens on the link-local address (which doesn't require RA), the missing RA processing suggests the IPv6 stack is minimal and may be missing essential features.

5. **🟢 UDP socket not bound to port 5540** — Less likely since the device successfully handles BLE-to-WiFi transition and advertises port 5540 in mDNS. But worth verifying the socket is bound before the CASE_Sigma1 arrives.

---

## Resolution

**Disabling "ARP-to-Unicast Conversion" on the router fixes the Rust device.**

With this router feature turned off, the Rust device successfully completes CASE and commissions via Apple Home. This confirms the issue is related to how the router handles multicast/unicast conversion for neighbor discovery traffic — the Rust device's minimal IPv6 stack cannot cope with the converted (unicast) frames, while the full lwIP stack in the C++ device handles them fine.

**Open question:** It remains a mystery why the C++ device works even with ARP-to-Unicast Conversion enabled. The lwIP stack in ESP-IDF presumably handles the converted frames correctly, but the exact mechanism — whether it's a difference in WiFi multicast filter configuration, lwIP's more robust ND implementation, or something else entirely — is unclear.

---

## What to Look For in the Code

The Rust device uses:

- `rs-matter` + `rs-matter-embassy` + `rs-matter-stack` for the Matter protocol stack
- `esp-hal` + `esp-wifi` for WiFi/BLE
- Embassy for async runtime

Key areas to investigate in the Rust stack:

1. **ICMPv6 handling** in `rs-matter-embassy` or `rs-matter-stack` — Is there ANY code that processes incoming ICMPv6 packets (type 135 = NS) and sends Neighbor Advertisements (type 136 = NA)?

2. **Multicast group management** — When the link-local IPv6 address is configured, does the code join the solicited-node multicast group `ff02::1:ff<last 24 bits>`? Check MLD join logic.

3. **smoltcp or custom IPv6 stack** — Does the Rust stack use `smoltcp` (which handles ND) or a custom minimal IPv6 implementation? If custom, ND is probably missing.

4. **WiFi multicast filter** — Check how `esp-wifi` configures multicast frame reception. Does it accept arbitrary multicast MACs or only a configured set?

5. **Netif state management** — The single-shot `NetifState` change at line 2896 suggests the Rust stack doesn't have the same event-driven IP lifecycle as lwIP. Check if this affects address readiness.

Compare with the C++ device's ESP-IDF which uses lwIP — a full-featured TCP/IP stack with complete IPv6 support (ND, SLAAC, DAD, MLD).

---

## Files

- `log-22-merged.plog` — Rust device session (FAILS) — 3431 lines
- `log-23-merged.plog` — C++ device session (SUCCEEDS) — 6323 lines

Both files are in the `intercom-matter/` directory. The Rust codebase is in `src/` and the patched dependency repos:

- `rs-matter` → `../rs-matter`
- `rs-matter-embassy` → `../rs-matter-embassy`
- `rs-matter-stack` → `../rs-matter-stack`
