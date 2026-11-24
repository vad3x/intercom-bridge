# Spec Delta

## MODIFIED Requirements

### Requirement: Reusable Driver Composition (`edf_matter::ipv6_compat`)
`edf-matter` SHALL provide `Ipv6CompatDriver<D>` and `wrap_ipv6_compat(driver)` wrapping any `embassy_net::driver::Driver` with `Mldv1ConvertingDriver` (TX) and `NeighborLearningDriver` (RX) before passing the driver to `rs_matter_embassy::enet::create_enet_stack`, requiring zero custom feature flags or patches in `rs-matter-embassy`.

#### Scenario: Firmware wraps station interface with `wrap_ipv6_compat`
- **WHEN** `intercom-firmware` initializes the ESP Wi-Fi station driver via `init_esp_network` (on `main`) or `v1` `main` network setup
- **THEN** it passes `edf_matter::ipv6_compat::wrap_ipv6_compat(esp_radio::wifi::Interface::station())` to `rs_matter_embassy::enet::create_enet_stack`

#### Scenario: `v1` Cargo manifests omit removed `rs-matter-embassy` IPv6 compatibility features
- **WHEN** `v1` `intercom-adapter-matter` and `intercom-firmware` declare their `rs-matter-embassy` dependency
- **THEN** they do not enable `mld-v1-compat`, `nd-learn-from-frames`, or `dhcpv4-hostname`
