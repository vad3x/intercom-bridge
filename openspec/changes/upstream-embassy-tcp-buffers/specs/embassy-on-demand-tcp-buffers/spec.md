# Spec Delta: On-Demand Embassy TCP Buffers for Matter Operational Runner

## Purpose

Optimizes Embassy network memory consumption for Matter-over-TCP by allocating TCP socket RX/TX buffers on demand on connection initiation and releasing them on teardown.

## ADDED Requirements

### Requirement: On-Demand TCP Buffer Allocation
The Embassy network runner SHALL allocate TCP RX/TX socket buffers dynamically when a Matter TCP connection is initiated, rather than reserving them statically in `.bss`/`.data` at boot.

#### Scenario: TCP buffers unallocated during idle standby
- **WHEN** the device boots and operates with no active Matter-over-TCP session
- **THEN** no static memory is reserved for inactive TCP socket buffers
- **AND** system SRAM remains fully available for audio DSP and Wi-Fi DMA

#### Scenario: Buffers released on TCP connection teardown
- **WHEN** an active Matter-over-TCP session terminates or times out
- **THEN** the allocated TCP RX and TX buffers are deallocated and returned to the system heap
