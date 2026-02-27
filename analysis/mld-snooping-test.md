# MLD Snooping Test Results

**Date:** 2026-02-25  
**Router setting:** MLD Snooping OFF  
**WiFi interface:** en0

## Test 1: `ping6 ff02::1` — All Nodes Multicast

```
ping6 -c 5 -I en0 ff02::1
```

3 devices responded (+ Mac itself as source):

| Address                       | Device          | Latency        |
| ----------------------------- | --------------- | -------------- |
| `fe80::6e4c:bcff:fe41:7339`   | Router          | ~4–9ms (stable)|
| `fe80::1c6a:fa48:149c:eb73`   | iPhone          | 16–305ms       |
| `fe80::cac:fea7:8682:4d60`    | Apple TV 4K     | 16–280ms       |

- 5 packets transmitted, 5 received, +7 duplicates, 0% loss
- ESP32 did not respond (normal — embedded stacks typically ignore ICMPv6 echo to multicast)

## Test 2: `ping6 ff02::fb` — mDNS Multicast Group

```
ping6 -c 5 -I en0 ff02::fb
```

2 devices responded:

| Address                       | Device          | Latency        |
| ----------------------------- | --------------- | -------------- |
| `fe80::1c6a:fa48:149c:eb73`   | iPhone          | 16–305ms       |
| `fe80::cac:fea7:8682:4d60`    | Apple TV 4K     | 16–243ms       |

- 5 packets transmitted, 4 received, +3 duplicates, 20% loss
- 20% loss is expected — `ff02::fb` is for UDP:5353 (mDNS), not ICMP echo

## Test 3: `tcpdump` — mDNS Traffic (UDP 5353, IPv6)

```
sudo tcpdump -i en0 -n udp port 5353 and ip6
```

5 distinct devices observed sending/receiving mDNS multicast:

| Address                       | Device                  | Activity                                      |
| ----------------------------- | ----------------------- | --------------------------------------------- |
| `fe80::56ef:44ff:fe80:5960`   | Aqara Hub M3            | Querying & responding `_matter._tcp`           |
| `fe80::cac:fea7:8682:4d60`    | Apple TV 4K (Home Hub)  | TXT records, `PTR _matter._tcp.local.`         |
| `fe80::1c6a:fa48:149c:eb73`   | iPhone                  | Querying AirPlay services                      |
| `fe80::9aa3:16ff:fea6:7844`   | **ESP32** (192.168.0.151) | Advertising Matter service on port 5540       |
| `fe80::420:519d:e431:3a18`    | Mac                     | Querying `_matter._tcp`                        |

ESP32 Matter advertisement observed:

```
SRV 98A316A67844.local.:5540
PTR DC86FEB5BB9F8FDF-000000006B84CD09._matter._tcp.local.
PTR _matter._tcp.local.
A   192.168.0.151
AAAA fe80::9aa3:16ff:fea6:7844
```

## Test 4: `dns-sd -B _matter._tcp local.` — Matter Service Browse

```
dns-sd -B _matter._tcp local.
```

Found 25+ Matter service instances across 4 fabric prefixes:

| Fabric Prefix      | Instances | Notes                       |
| ------------------- | --------- | --------------------------- |
| `47F49EDF43015297`  | 11        | Controller fabric           |
| `DC86FEB5BB9F8FDF`  | 11        | Controller fabric           |
| `048BDC4364465DE2`  | 3         | Aqara Hub                   |
| `6AA61A6C3A93F404`  | 1         | Single device               |

ESP32 instance discovered: `DC86FEB5BB9F8FDF-000000006B84CD09`

## Conclusions

| Check                                  | Result |
| -------------------------------------- | ------ |
| MLD Snooping off (multicast flooding)  | ✅ Confirmed — traffic from all devices visible |
| ESP32 mDNS advertisement               | ✅ Working — service, address, port all correct |
| Apple TV sees Matter advertisements     | ✅ Confirmed — responding to `_matter._tcp` |
| `dns-sd` browse finds ESP32            | ✅ Discovered successfully |
| Multicast as root cause of issues       | ❌ Ruled out — multicast delivery is working |

**If commissioning issues persist, the problem is not at the multicast/network discovery layer — it occurs later in the Matter session (CASE/PASE handshake, subscriptions, etc.).**
