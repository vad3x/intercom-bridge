# IPv6 MLD Report Fix for Matter Commissioning

## Status: **IN PROGRESS ⚠️ — MLDv2→MLDv1 + deferred IPv6 + gratuitous NA, still unreliable**

## Problem

Matter commissioning over WiFi experiences a ~50-60 second timeout during CASE (Certificate Authenticated Session Establishment). The iPhone discovers both IPv6 and IPv4 addresses via mDNS and prefers IPv6 (score 7 vs 2). When the iPhone sends CASE Sigma1 to the ESP32-C6's link-local IPv6 address (`fe80::9aa3:16ff:fea6:7844:5540`), the device never receives it. After ~50-60s the iPhone times out and retries over IPv4, which works immediately.

## Root cause: Omada AP does not forward IPv6 unicast to the device

The TP-Link Omada AP (EAP653) does not forward IPv6 unicast or NDP Neighbor Solicitation to the ESP32-C6 running Rust/smoltcp firmware. The device only ever receives **multicast** IPv6 frames (mDNS at `ff02::fb`, MLD reports from other devices) — **zero unicast IPv6** frames are observed in RX logs.

### Why smoltcp specifically?

**smoltcp sends MLDv2 Reports (type 143), but the Omada AP only processes MLDv1 Reports (type 131).** Without seeing an MLDv1 Report for the solicited-node multicast group (`ff02::1:ffa6:7844`), the AP never learns to forward NDP Neighbor Solicitation to the device, and IPv6 unicast is dead.

The **same ESP32-C6 hardware with C++ firmware (lwIP) works perfectly** — lwIP sends MLDv1 Reports (type 131). This was the definitive clue that led to the MLDv2→MLDv1 conversion fix.

## Evidence

### C++ firmware (lwIP) — ✅

- CASE Sigma1 → Sigma2: **0.85 seconds** over IPv6
- Omada shows IPv6 address ✅
- Sends MLDv1 Reports (type 131) ✅

### Rust firmware before fix (smoltcp) — ❌

- CASE Sigma1 → **57 second timeout** over IPv6
- Falls back to IPv4 (works instantly)
- Omada shows **no** IPv6 address ❌
- Sends MLDv2 Reports (type 143) ❌

### Rust firmware with MLDv1 conversion — ⚠️ intermittent

- MLDv2→MLDv1 conversion active ✅
- Deferred IPv6 config (applied only after link up) ✅
- Gratuitous NAs sent (3 rounds: 2s, 7s, 12s after link up) ✅
- **Sometimes works** (log102/103), **usually doesn't** (log105–112, log-1)
- Logs 102/103 "success" was due to AP having **cached MLD state from prior C++ firmware run** — not from the MLDv1 conversion itself

## Current implementation

### 1. MLDv2→MLDv1 in-place conversion

**File:** `rs-matter-embassy/rs-matter-embassy/src/enet.rs`

The `Ipv6LoggingTxToken` intercepts outgoing MLDv2 Reports and rewrites them as MLDv1:

1. Detects MLDv2 Report (ICMPv6 type 143, dst `ff02::16`)
2. Extracts the multicast group address from the first MLDv2 record
3. Rewrites as MLDv1 Report (type 131, dst = group multicast MAC)
4. Recalculates ICMPv6 checksum
5. Shrinks the frame to MLDv1 size (90 bytes)

### 2. Deferred IPv6 configuration

**Files:** `rs-matter-embassy/rs-matter-embassy/src/enet.rs`, `wireless/wifi.rs`

IPv6 is not configured at boot. Instead, `EnetNetif::wait_changed()` applies the IPv6 configuration only when the WiFi link is up. This ensures smoltcp's automatic MLD join for the solicited-node multicast group is transmitted over the air.

### 3. Gratuitous Neighbor Advertisements

After IPv6 is configured, 3 unsolicited Neighbor Advertisements are sent at 2s, 7s, and 12s after link-up. The goal is to populate the AP's NDP proxy table even if MLD wasn't processed. **This has not been proven to help.**

## What didn't work (investigation history)

1. **MLD re-join after WiFi connect** — `smoltcp` `join_multicast_group` is a no-op if group is already `Joined`
2. **Leave → delay → rejoin** — `has_multicast_group()` never returned `false` after leave
3. **Disabling MLD Snooping on AP** — didn't help (AP may still filter MLDv2 internally)
4. **Disabling Multicast-to-Unicast Conversion** — didn't help
5. **AAAA suppression hack** — worked as a workaround (forces IPv4-only) but hides the real problem
6. **MLD retries at 5-second intervals** (toggle IPv6 off→on to re-send MLD reports) — didn't help, each cycle re-triggers DAD
7. **Gratuitous Neighbor Advertisements** — didn't help (AP doesn't seem to learn from unsolicited NAs)
8. **MLD retry at 1 second** (quick toggle) — didn't help (AP not ready so soon after association)

## Key observation: log-1-device.log analysis

The device sends correct MLDv1 Reports right after link up:
- `00:00:48.808` — MLDv1 Report for `ff02::1:ffa6:7844` (solicited-node) ✅
- `00:00:48.815` — MLDv1 Report for `ff02::fb` (mDNS multicast) ✅

The device receives multicast IPv6 from other devices on the network (mDNS queries, MLD Reports from the second ESP32 at `98:a3:16:a6:79:3c`, Router Advertisements) — but **never** receives:
- NDP Neighbor Solicitation for its address
- Any unicast IPv6 frame
- Any IPv6 frame with `dst_mac=98:a3:16:a6:78:44`

The iPhone (iphone log-1) sends CASE Sigma1 to `fe80::9aa3:16ff:fea6:7844:5540` at `14:51:11.098`, retransmits 5 times, and gives up. The packet never arrives at the device.

## Timeline

| Log | What was tried | Result |
|-----|---------------|--------|
| new-log86 | Removed AAAA hack | IPv6 broken, CASE timeout |
| new-log87 | MLD rejoin (wrong timing) | Still broken |
| new-log88 | MLD rejoin (correct timing) | Still broken |
| new-log89 | Leave→poll→rejoin | Leave never completed |
| new-log91 | Clean state, real timing | Confirmed IPv6 completely dead |
| new-log93 | Custom IPv6 logging driver | Only multicast RX, no unicast |
| new-log97 | Deferred IPv6 config | MLD sent, but MLDv2 → AP ignores |
| new-log98 | More logging | Confirmed MLDv2 sent after link up |
| new-log99 | TX logging added | Discovered MLDv2 vs MLDv1 mismatch |
| new-log100 | AP settings disabled | Still broken (AP ignores MLDv2 regardless) |
| **new-log102** | **MLDv2→MLDv1 conversion** | **✅ Worked (but due to cached C++ MLD state)** |
| **new-log103** | **Same code, 2nd run** | **✅ Worked (still cached state)** |
| new-log105 | Same code, fresh boot | ❌ IPv6 dead again |
| new-log106 | MLD retry at 1s | ❌ Still broken |
| new-log108 | Reverted to working code | ❌ Still broken (BLE panic) |
| new-log110 | Same, multiple BLE panics | ❌ Still broken |
| new-log112 | MLD retries at 5s intervals | ❌ Still broken |
| log-0 | Gratuitous NAs | ❌ SendNOC timeout (complete failure) |
| log-1 | Gratuitous NAs (3 rounds) | ❌ IPv6 CASE timeout, only multicast RX |

## Open questions

1. **Why does the AP cache C++ firmware's MLD state but not the Rust firmware's?** Both send MLDv1 Reports with the same source IP and for the same multicast groups. Is there a timing difference? Does lwIP send additional frames (e.g., NDP Router Solicitation, gratuitous ARP/NA) that "register" the device with the AP?

2. **Is the Omada AP using some mechanism beyond MLD Snooping?** Even with MLD Snooping "OFF" in the UI, the AP may still be doing bridge-level multicast filtering or NDP proxying.

3. **Does the ESP32 WiFi firmware (`esp-radio`) require specific multicast MAC filter configuration?** The WiFi hardware may be filtering at the MAC level. The C++ firmware (ESP-IDF) may configure the WiFi driver's multicast filter list differently than `esp-radio`.

## Possible next steps

1. **Compare WiFi multicast filter setup** between C++ ESP-IDF and Rust `esp-radio` — the hardware may be dropping frames at the WiFi MAC level
2. **Try a different AP** to isolate the issue — if it works on a different AP, the issue is definitively AP-specific
3. **Restore the AAAA suppression hack** as a reliable workaround while investigating further
4. **Patch smoltcp to send MLDv1 natively** instead of converting at the driver level — eliminate any timing/ordering issues
5. **Send NDP Router Solicitation** after link up — lwIP does this, and it may be what triggers the AP to "register" the device
