# embassy-ipv6-compat Specification

## Purpose

Provides reusable `embassy-net` driver wrappers (`Mldv1ConvertingDriver` and `NeighborLearningDriver` via `edf_matter::ipv6_compat::wrap_ipv6_compat`) in `embedded-device-foundation` (`edf-matter`) to ensure reliable IPv6 multicast snooping and unicast neighbor cache learning on Wi-Fi networks across Intercom and HVAC firmware without patching `rs-matter-embassy`.

## Requirements

### Requirement: Reusable Driver Composition (`edf_matter::ipv6_compat`)
`edf-matter` SHALL provide `Ipv6CompatDriver<D>` and `wrap_ipv6_compat(driver)` wrapping any `embassy_net::driver::Driver` with `Mldv1ConvertingDriver` (TX) and `NeighborLearningDriver` (RX) before passing the driver to `rs_matter_embassy::enet::create_enet_stack`, requiring zero custom feature flags or patches in `rs-matter-embassy`.

#### Scenario: Firmware wraps station interface with `wrap_ipv6_compat`
- **WHEN** `intercom-firmware` initializes the ESP Wi-Fi station driver via `init_esp_network`
- **THEN** it passes `edf_matter::ipv6_compat::wrap_ipv6_compat(esp_radio::wifi::Interface::station())` to `rs_matter_embassy::enet::create_enet_stack`

### Requirement: MLDv2 to MLDv1 TX Conversion (`Mldv1ConvertingDriver`)
`Mldv1ConvertingDriver` SHALL rewrite outgoing single-record `MODE_IS_EXCLUDE` / `CHANGE_TO_EXCLUDE` MLDv2 Membership Reports (ICMPv6 type 143) into valid RFC 2710 MLDv1 Membership Reports (ICMPv6 type 131) with updated IPv6 length and ICMPv6 checksum, leaving all other frames untouched.

#### Scenario: Outgoing MLDv2 join report
- **WHEN** `smoltcp` transmits an Ethernet frame carrying a single-group MLDv2 join report for `ff02::1:ffa6:7844`
- **THEN** `Mldv1ConvertingDriver` transmits an MLDv1 Membership Report (type 131) for `ff02::1:ffa6:7844` with IPv6 payload length 32 and a valid ICMPv6 checksum

### Requirement: Passive Unicast Neighbor Learning (`NeighborLearningDriver`)
`NeighborLearningDriver` SHALL inspect incoming unicast IPv6 Ethernet frames and, upon observing a new or updated `(src_ip, src_mac)` pair, yield a synthetic Neighbor Solicitation frame to `smoltcp` on the next receive poll.

#### Scenario: First unicast IPv6 frame from an un-cached peer
- **WHEN** an incoming unicast IPv6 UDP/TCP frame arrives from a peer MAC/IP not yet in `NeighborLearningDriver`'s table
- **THEN** `receive` delivers the data frame and yields a valid 86-byte synthetic Neighbor Solicitation with SLLA option for `(src_ip, src_mac)` on the next poll
