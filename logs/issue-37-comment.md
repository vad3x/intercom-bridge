## Root Cause Analysis: WiFi Commissioning Failure

I've been debugging the same issue on my ESP32-C6 setup (rs-matter + rs-matter-embassy, WiFi, Apple Home on iPhone). Here's what I found.

### Summary

The BLE commissioning (PASE) completes successfully, but the subsequent **CASE session over WiFi** fails because the iPhone's **IPv6 Sigma1 packets never reach the device**. The iPhone prefers IPv6 (link-local) over IPv4 and spends ~57 seconds retrying before falling back to IPv4. By then, the Matter **fail-safe timer has expired** (Apple uses a 30-second fail-safe for the System Commissioner step), and the device rejects the `CommissioningComplete` command.

### Detailed Flow

```
iPhone ──BLE──> ESP32    PASE session ✅
iPhone ──BLE──> ESP32    AddNOC (iPhone fabric) ✅
iPhone ──BLE──> ESP32    WiFi credentials ✅
  (ESP32 connects to WiFi, starts mDNS advertising)
iPhone ──BLE──> ESP32    AddNOC (System Commissioner / Apple TV Hub fabric) ✅
  (fail-safe armed for 30 seconds)
iPhone ──WiFi/IPv6──> ESP32    CASE Sigma1 ❌ never arrives!
  (5 retransmissions over ~16 seconds, all lost)
  (~41 second timeout waiting for CASE response)
iPhone ──WiFi/IPv4──> ESP32    CASE Sigma1 ✅ (fallback)
iPhone <──WiFi/IPv4── ESP32    CASE Sigma2 ✅
iPhone ──WiFi/IPv4──> ESP32    CASE Sigma3 ✅
iPhone ──WiFi/IPv4──> ESP32    CommissioningComplete ❌ fail-safe expired!
  "Pairing with accessory failed."
```

### Same Hardware Works with C++ / ESP-IDF (lwIP)

The **exact same ESP32-C6 board**, same WiFi AP, same iPhone, same network — commissions flawlessly with the C++ ESP-IDF Matter SDK. Both CASE sessions complete over IPv6 in under 1 second:

**C++ (ESP-IDF + lwIP) — SUCCESS:**

```
12:28:52.260  WiFiNetworkEnable completed
12:28:54.807  Sigma1 TX → [UDP:[fe80::9aa3:16ff:fea6:7844%en0]:5540]   ← IPv6
12:28:55.653  Sigma2 RX (0.8s later)                                    ← works!
12:28:55.658  Sigma3 TX
12:28:56.656  Session established (FabricIndex 2)
12:28:57.065  CommissioningComplete ✅

12:29:02.115  Sigma1 TX → [UDP:[fe80::9aa3:16ff:fea6:7844%en0]:5540]   ← IPv6 again
12:29:02.643  Sigma2 RX (0.5s later)                                    ← works!
12:29:03.334  Session established (FabricIndex 1)                        ← System Commissioner ✅
```

**Rust (rs-matter + smoltcp) — FAILURE:**

```
16:16:31.235  WiFiNetworkEnable completed
16:16:31.963  Sigma1 TX → [UDP:[fe80::9aa3:16ff:fea6:7844%en0]:5540]   ← IPv6
              (5 retransmissions, all lost)
16:17:29.195  CASESession timed out (57 seconds!)                       ← fails!
16:17:29.198  Sigma1 TX → [UDP:192.168.0.151:5540]                     ← IPv4 fallback
16:17:30.296  Sigma2 RX (1.1s later)                                    ← IPv4 works
16:17:32.492  CommissioningComplete → fail-safe expired ❌
```

The C++ device-side logs confirm both CASE sessions are received and handled over IPv6:

```
I (32389) chip[EM]: >>> Msg RX ... CASE_Sigma1 (B:196)     ← fabric 1, IPv6 unicast arrives
I (32782) chip[EM]: <<< Msg TX ... CASE_Sigma2 (B:529)
I (34243) chip[FS]: GeneralCommissioning: Received CommissioningComplete
I (34286) chip[FS]: Fail-safe cleanly disarmed

I (39463) chip[EM]: >>> Msg RX ... CASE_Sigma1 (B:196)     ← fabric 2, IPv6 unicast arrives
I (39845) chip[EM]: <<< Msg TX ... CASE_Sigma2 (B:534)
I (40702) chip[FS]: GeneralCommissioning: Received CommissioningComplete
I (40750) chip[FS]: Fail-safe cleanly disarmed
```

This rules out the WiFi AP, network environment, and iPhone as the cause. **The problem is in the Rust network stack (`smoltcp` / `embassy-net`)**, specifically in how it handles IPv6 NDP neighbor discovery compared to ESP-IDF's `lwIP` (probably).

### Why IPv6 Packets Don't Arrive (My Speculation)

The iPhone resolves the device via mDNS and gets both an AAAA record (IPv6 link-local `fe80::...`) and an A record (IPv4). It prefers IPv6 (score 7 vs score 2 for IPv4).

To send a unicast IPv6 packet, the iPhone first needs to resolve the device's MAC address via **NDP (Neighbor Discovery Protocol)**. It sends a **Neighbor Solicitation** to the **solicited-node multicast group** `ff02::1:ffXX:XXXX`.

This is where it breaks. There are two possible failure modes depending on your network:

**1. MLD Snooping on the AP/router (most common)**

Most consumer WiFi routers have **MLD Snooping** enabled by default. MLD Snooping optimizes multicast by only forwarding traffic to clients that explicitly joined a multicast group via MLD reports. The ESP32's embedded TCP/IP stack (`smoltcp`) does send MLD join messages, but they can be:

- Sent too late (after the iPhone already tried NS)
- Dropped by the AP due to timing or implementation bugs
- Not recognized for the solicited-node multicast group

Result: the NS never reaches the ESP32, the iPhone can't resolve the MAC, and all IPv6 unicast fails silently.

**2. smoltcp neighbor cache (secondary issue)**

Even when multicast delivery works (MLD snooping off), `smoltcp` has a strict NDP implementation — it only learns neighbor MAC addresses from NDP messages, not from incoming data frames. So even if a unicast IPv6 packet somehow arrives, `smoltcp` may not be able to reply because it doesn't know the sender's MAC address. It sends its own NS to resolve the sender, which may also get lost.

### Evidence

From device logs — between WiFi coming up and commissioning failure, the device receives **hundreds of multicast packets** (`dst=33:33:...`) but **zero unicast IPv6 packets**:

```
IPv6 RX: 113 bytes, proto=UDP dst=33:33:00:00:00:fb ...  ← mDNS multicast (works)
IPv6 RX: 86 bytes, ICMPv6 type=131 dst=33:33:00:00:00:02 ...  ← MLD reports (works)
(no unicast IPv6 RX at all)
```

From iPhone logs — CASE Sigma1 is sent over IPv6, retried 5 times, then falls back to IPv4:

```
[UDP:[fe80::9aa3:16ff:fea6:7844%en0]:5540] --- CASE_Sigma1    ← IPv6, no response
Msg Retransmission failure (max retries:4)
CASESession timed out ...
[UDP:192.168.0.151:5540] --- CASE_Sigma1                      ← IPv4 fallback, works
Session was established
CommissioningComplete → "Fail-Safe Disarm without it being armed"  ← too late!
```

### Workaround: Suppress IPv6 (AAAA) in mDNS

The most reliable workaround I found is to **not advertise the IPv6 address in mDNS responses**. This forces all Matter controllers to use IPv4, which uses ARP (broadcast) for address resolution and works reliably on all networks.

In `rs-matter-stack/src/mdns.rs`, pass `Ipv6Addr::UNSPECIFIED` instead of the real IPv6 address to the `Host` struct:

```rust
// In BuiltinMdns::run(), change the Host construction:
BuiltinMdnsResponder::new(matter, crypto, notify)
    .run(
        udp::Udp(send),
        udp::Udp(recv),
        &Host {
            id: 0,
            hostname: &hostname,
            ip: ipv4,
            ipv6: Ipv6Addr::UNSPECIFIED,  // ← suppress AAAA record
        },
        Some(ipv4),
        Some(interface),
    )
    .await?;
```

This only suppresses the AAAA record in mDNS responses. The mDNS socket itself still listens on IPv6 multicast (bound to `::` and joined to `ff02::fb`), so IPv6 mDNS queries are still received and answered — they just won't include AAAA records, forcing clients to use the A record (IPv4).

### Environment

- ESP32-C6
- `rs-matter` + `rs-matter-embassy` + `rs-matter-stack` (Rust, bare-metal, Embassy)
- iPhone 15, iOS 18
- Apple TV 4K as Home Hub
- Verizon router (MLD Snooping enabled by default)
