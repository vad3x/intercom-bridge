# Spec Delta: Configurable App Sockets & Stack Access (`embassy-app-sockets`)

## Purpose

Allows applications using `rs-matter-embassy` to reserve spare TCP and UDP sockets in `embassy_net::StackResources` and obtain a handle to the initialized `embassy_net::Stack`.

## ADDED Requirements

### Requirement: Configurable Extra TCP and UDP Socket Capacity
`rs-matter-embassy` SHALL allow reserving additional application TCP and UDP sockets in `ENET_MIN_SOCKET_SET` (supporting at least 2 extra TCP sockets and 2 extra UDP sockets for concurrent HTTP/WS/OTA and WebRTC).

#### Scenario: Application binds concurrent TCP and UDP sockets
- **WHEN** `intercom-firmware` configures 2 extra TCP sockets and 2 extra UDP sockets
- **THEN** `EnetMatterStackResources` sizes `StackResources<ENET_MIN_SOCKET_SET>` to hold Matter sockets, DHCP/DNS, plus the configured extra app sockets

### Requirement: Application Access to `embassy_net::Stack`
`rs-matter-embassy` SHALL expose a mechanism for application tasks to obtain the initialized `embassy_net::Stack` once the network interface is created.

#### Scenario: Independent HTTP and WebRTC tasks wait for network stack
- **WHEN** `create_enet_stack` initializes the `embassy_net::Stack`
- **THEN** application tasks can obtain the `Stack` handle without blocking Matter's own event loop
