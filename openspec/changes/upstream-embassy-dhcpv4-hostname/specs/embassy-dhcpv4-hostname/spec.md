# Spec Delta: Opt-In DHCPv4 Hostname (Option 12) Support

## Purpose

Allows `rs-matter-embassy` consumers to opt into `embassy-net`'s `dhcpv4-hostname` feature and configure a DHCPv4 Option 12 client hostname on Ethernet and Wi-Fi stacks.

## ADDED Requirements

### Requirement: Opt-In `dhcpv4-hostname` Feature Flag
`rs-matter-embassy` SHALL define a non-default `dhcpv4-hostname` Cargo feature that forwards `embassy-net?/dhcpv4-hostname` without altering behavior when the feature is disabled.

#### Scenario: Compiling without `dhcpv4-hostname`
- **WHEN** `rs-matter-embassy` is built with default features
- **THEN** `dhcpv4-hostname` is not enabled and `create_enet_config` leaves `DhcpConfig::default()` unchanged

### Requirement: DHCPv4 Hostname Configuration & Builders
When `dhcpv4-hostname` is enabled, `rs-matter-embassy` SHALL provide `create_enet_config_with_hostname`, `create_enet_stack_with_hostname`, and `const fn with_hostname(&'a str)` on `EmbassyEthernet`, `EmbassyWifi`, and `EnetWifiRunner` that populate `DhcpConfig.hostname` for non-empty hostnames up to 32 bytes and ignore empty or >32-byte hostnames without panicking.

#### Scenario: Valid hostname up to 32 bytes
- **WHEN** `create_enet_config_with_hostname(&mac, Some("intercom-bridge"))` (or `.with_hostname("intercom-bridge")`) is called
- **THEN** the resulting `embassy_net::Config` carries DHCPv4 Option 12 hostname `"intercom-bridge"`

#### Scenario: Empty or oversized hostname (>32 bytes)
- **WHEN** `create_enet_config_with_hostname` is called with `Some("")` or a string longer than 32 bytes
- **THEN** `DhcpConfig.hostname` remains `None` and the call succeeds without panicking
