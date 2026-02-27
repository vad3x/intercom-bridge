# Matter Commissioning Delay Analysis (Log #74)

## Summary

Total commissioning time: **~4 min 31s** (19:04:01 → 19:08:32).

The iPhone log reveals **two separate commissioning attempts**. The first failed at WiFiNetworkEnable; the second succeeded but was delayed by an IPv6 CASE timeout. The device log (`new-log74.log`) covers only the second (successful) attempt.

## Overall Timeline

```
19:04:01  ── 1st attempt starts (PASE over BLE, node 3FB617A8)
19:04:06  ── PASE established (~5.2s, includes 4.6s SPAKE2+)
19:04:07  ── Commissioning steps begin
19:04:13  ── SendNOC done, WiFiNetworkSetup done
19:04:18  ── ConnectNetwork FAILED (networkingStatus=9) ← 🔴 FIRST FAILURE
              ... ~1 min 55s gap (device reset + user retry) ...
19:06:13  ── 2nd attempt starts (PASE over BLE, node 4914B059)
19:06:18  ── PASE established (~5.2s, includes 4.6s SPAKE2+)
19:06:24  ── SendNOC done, WiFi connected
19:06:28  ── iPhone sends CASE Sigma1 to IPv6 link-local
              ... device never responds ...
19:08:29  ── IPv6 TIMEOUT (~2 min), iPhone retries with IPv4  ← 🔴 BIGGEST DELAY
19:08:30  ── Sigma2 received via IPv4 (~1s)
19:08:32  ── CommissioningComplete ✅
```

## Bottleneck #1: IPv6 CASE Timeout (~2 minutes)

**Impact:** ~120 seconds — the single biggest delay.

After WiFi connects, the iPhone resolves the device via mDNS and finds two addresses:

```
Mdns: OnNewAddress ... ip: fe80::9aa3:16ff:fea6:7844   ← IPv6 link-local (score: 7, preferred)
Mdns: OnNewAddress ... ip: 192.168.0.144                ← IPv4 (score: 2, fallback)
```

The iPhone sends CASE Sigma1 to IPv6 first (per Matter spec — higher score):

```
19:06:28.827  <<< Msg TX ... [UDP:[fe80::9aa3:16ff:fea6:7844%en0]:5540] --- CASE_Sigma1
```

**The device never receives this packet.** After ~2 minutes, the iPhone times out and retries with IPv4:

```
19:08:29.493  <<< Msg TX ... [UDP:192.168.0.144:5540] --- CASE_Sigma1   ← works instantly
19:08:29.493  Session establishment failed ... CHIP Error 0x00000032: Timeout.
```

On the device side, the gap is clearly visible — only mDNS traffic between WiFi up and CASE arrival:

```
00:00:44.596  Wifi connected
... 2 min 2s of only mDNS queries ...
00:02:46.910  >>RCV UDP 192.168.0.146:51591 ... SC::CASESigma1   ← IPv4 retry
```

**Root cause:** The ESP32-C6's `rs-matter-embassy` WiFi stack does not receive unicast UDP on the IPv6 link-local address (port 5540), despite:

- Having the correct IPv6 link-local address (derived from MAC `98:a3:16:a6:78:44`)
- Successfully sending/receiving mDNS multicast on IPv6 (`[ff02::fb]:5353`)
- Advertising the IPv6 address in mDNS responses

**Fix options:**

1. Fix `rs-matter-embassy` WiFi transport to bind UDP on both IPv4 and IPv6 on port 5540
2. Or stop advertising AAAA records in mDNS so the iPhone goes straight to IPv4

## Bottleneck #2: First Attempt ConnectNetwork Failure (~1 min 55s wasted)

**Impact:** ~115 seconds (failed attempt + gap + second PASE).

The first attempt completes all steps up to WiFiNetworkEnable, then fails:

```
19:04:18.885  Received ConnectNetwork response, networkingStatus=9
19:04:18.885  Error on commissioning step 'WiFiNetworkEnable': CHIP Error 0x000000AC: Internal error
19:04:18.885  Going from 'WiFiNetworkEnable' -> 'Cleanup'
```

`networkingStatus=9` is `UnknownError` in the Matter NetworkCommissioning cluster spec. This forces a full restart of the commissioning flow with a new node ID.

**Root cause:** Likely a race condition in the NetworkCommissioning cluster handler — the ConnectNetwork response may be sent before WiFi connection status is confirmed, or the first WiFi connection attempt fails transiently.

**Fix:** Investigate the ConnectNetwork handler in `rs-matter-embassy`'s WiFi networking driver to ensure the response correctly reflects the actual connection state.

## Bottleneck #3: SPAKE2+ Computation (~4.6s per attempt)

**Impact:** ~9.2s total (ran twice due to retry).

```
00:00:35.963  PASE Pake1→Pake2 (SPAKE2+ verify) took 4627ms
```

This is CPU-bound elliptic curve cryptography on the ESP32-C6 RISC-V core. Expected for software crypto on this hardware class.

**Fix options:**

- Enable hardware crypto acceleration if available on ESP32-C6
- Accept as inherent to the platform (~5s is within spec tolerances)

## Bottleneck #4: CASE Crypto (~2.7s)

**Impact:** ~2.7s (one-time, second attempt only).

```
00:02:47.983  CASE Sigma1→Sigma2 took 1067ms
00:02:49.670  CASE Sigma3 verification took 1685ms
```

Same root cause as SPAKE2+ — CPU-limited elliptic curve operations.

## Minor Delays

| Step | Duration | Notes |
|------|----------|-------|
| AddNOC | 978ms | Certificate storage + crypto |
| SendOpCertSigningRequest | ~1.4s | CSR generation on device |
| Attestation override | ~1.0s | AttestationVerification fails, iPhone overrides |
| BLE discovery to PASE start | ~0.3s | Normal |
| ReadCommissioningInfo | ~0.3s | Normal |
| ArmFailSafe + Config | ~0.2s | Normal |
| Certificate requests (PAI + DAC) | ~0.5s | Normal |

## Time Budget Summary

| Bottleneck | Time | Category |
|---|---|---|
| IPv6 CASE Sigma1 timeout | **~120s** | Bug — fixable |
| First attempt failure + retry | **~115s** | Bug — fixable |
| SPAKE2+ (×2 attempts) | **~9.2s** | Hardware-limited |
| CASE crypto | **~2.7s** | Hardware-limited |
| AddNOC + CSR | **~2.4s** | Hardware-limited |
| Everything else (normal flow) | **~22s** | Normal |
| **Total** | **~271s (~4m 31s)** | — |

## Expected Time After Fixes

If both bugs are fixed (IPv6 listening + ConnectNetwork response):

- Single attempt, IPv4 CASE: **~20–25 seconds**
- Breakdown: PASE (~5.2s) + commissioning steps (~6s) + WiFi connect (~2.3s) + CASE (~2.7s) + attribute sync (~4s) + overhead (~3s)

## Key Log Cross-References

### Clock Synchronization

Device uptime `00:00:00` ≈ iPhone `19:05:42` (second attempt boot).

Verified by matching:

- Device `00:00:42.049` (AddNOC) ↔ iPhone `19:06:24.738` (SendNOC done)
- Device `00:00:44.596` (WiFi connected) ↔ iPhone `19:06:27.254` (ConnectNetwork success)
- Device `00:02:46.910` (CASE Sigma1 RCV) ↔ iPhone `19:08:29.493` (Sigma1 TX via IPv4)

### iPhone Node IDs

- First attempt: `0x000000003FB617A8` (failed)
- Second attempt: `0x000000004914B059` (succeeded)

### Device Addresses

- MAC: `98:a3:16:a6:78:44`
- IPv6 link-local: `fe80::9aa3:16ff:fea6:7844` (EUI-64 derived, advertised but non-functional for unicast UDP)
- IPv4: `192.168.0.144` (functional)
- iPhone source: `192.168.0.146`
