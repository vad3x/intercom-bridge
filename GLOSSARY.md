# Domain Glossary

Ubiquitous vocabulary and architectural concepts for the **Intercom Bridge** project.

## Core Concepts (Hexagonal Domain)

### Intercom Bridge
The overarching system that bridges building intercom systems to modern smart home ecosystems. It is protocol-agnostic and hardware-agnostic: any intercom hardware adapter (e.g. IR204E, Siedle, Comelit) can be bridged to any smart home protocol adapter (e.g. Matter over Wi-Fi/Thread, Home Assistant, MQTT).

### Intercom Core (`intercom-core`)
The central, hardware-agnostic and protocol-agnostic domain and application orchestrator. Coordinates door unlocking, ring detection, indicator feedback, auto-relock timeouts, and factory reset routines. Zero dependencies on microcontroller peripherals or network stacks.

### Actor
An autonomous async task communicating exclusively via the event bus and port interfaces:
- **RingActor**: Listens to ring trigger events, applies debounce and cooldown policies, and announces ring events.
- **DoorActor**: Subscribes to unlock dispatch events and drives the door unlock pulse duration.
- **IndicatorActor**: State machine mapping system lifecycle states and transient events to physical LED blink patterns.
- **AccessControlActor**: Coordinates security timing rules: auto-relock timers and occupancy sensor state holding.
- **FactoryResetActor**: Monitors user maintenance button holds to trigger factory credential wiping.

### Event Bus
The broadcast communication channel (`PubSubChannel`) providing event fan-out between actors without direct coupling.

## Ports (Seams)

### Outbound Ports
Abstract async traits defined by `intercom-core` representing required capabilities:
- **RingDriver**: Waits for hardware ring signal detection.
- **DoorDriver**: Triggers the hardware relay unlock pulse.
- **IndicatorDriver**: Controls hardware visual status indicators.
- **AccessControlDriver**: Bridges cluster/ecosystem state (occupancy bit, lock status).
- **FactoryResetDriver**: Waits for factory reset button hold triggers.

### Inbound Ports
Entry points or commands dispatched from external smart home protocols into the core (e.g., Matter `Unlock` command calling the unlock trigger).

## Adapters (Edge Implementations)

### Intercom Hardware Adapter (`intercom-adapter-gpio`)
Concrete hardware drivers binding port traits to physical intercom lines and ESP32-C6 GPIO pins:
- `GpioRingDriver` (optocoupler sensing ring voltage, e.g. IR204E AC/DC buzzer line)
- `GpioDoorDriver` (PhotoMOS relay closing intercom door strike line)
- `GpioLedDriver` (active-low status LED)
- `GpioBootButtonDriver` (active-low maintenance button)
- `EspFlash` (ROM SPI flash persistence)

### Smart Home Protocol Adapter (`intercom-adapter-matter`)
Concrete protocol drivers binding port traits and events to the smart home ecosystem:
- `IntercomDoorLock` (Matter DoorLock cluster handler)
- `IntercomOccupancy` (Matter OccupancySensing cluster handler)
- `MatterAccessControlDriver` (bridges core lock/occupancy state)
- `MatterController` (Wi-Fi, BLE commissioning, and CASE session runner)
*(Future adapters: Home Assistant native API, MQTT, REST)*

## Composition Root (`intercom-firmware` -> binary: `intercom-bridge`)
The bootstrapping executable for a specific hardware target (ESP32-C6) that initializes peripherals, instantiates the chosen hardware adapter (`intercom-adapter-gpio`) and smart home adapter (`intercom-adapter-matter`), injects them into `intercom_core::App`, and runs the executor.
