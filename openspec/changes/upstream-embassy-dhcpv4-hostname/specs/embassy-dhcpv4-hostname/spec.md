# Spec Delta: Opt-in DHCPv4 Hostname (`dhcpv4-hostname`)

## Purpose

Allows `rs-matter-embassy` Ethernet and Wi-Fi runners to configure DHCPv4 Option 12 hostnames when the `dhcpv4-hostname` Cargo feature is enabled.

## ADDED Requirements

### Requirement: Opt-in DHCPv4 Hostname Builder API
When the `dhcpv4-hostname` feature is enabled, `EmbassyEthernet` and `EmbassyWifi` SHALL expose a `const fn with_hostname(self, hostname: &'a str) -> Self` builder method.

#### Scenario: Configuring a hostname on Wi-Fi or Ethernet
- **WHEN** `dhcpv4-hostname` is enabled and `.with_hostname("intercom-bridge")` is called
- **THEN** `create_enet_stack_with_hostname` sets `DhcpConfig::hostname` to the configured hostname

### Requirement: Default Build Isolation
When the `dhcpv4-hostname` feature is disabled, `rs-matter-embassy` SHALL compile without enabling `embassy-net/dhcpv4-hostname` or altering existing `EmbassyEthernet` / `EmbassyWifi` structs.

#### Scenario: Default feature build
- **WHEN** `rs-matter-embassy` is built without `dhcpv4-hostname`
- **THEN** `DhcpConfig` uses upstream defaults with no hostname overhead
