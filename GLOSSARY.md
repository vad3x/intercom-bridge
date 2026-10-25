# Domain Glossary

Ubiquitous vocabulary and architectural concepts for the **Intercom Bridge** project.

## Core Concepts (Hexagonal Domain)

### Intercom Bridge
The overarching system that bridges building intercom systems to modern smart home ecosystems. It is protocol-agnostic and hardware-agnostic: any intercom hardware adapter (e.g. IR204E, Siedle, Comelit) can be bridged to any smart home protocol adapter (e.g. Matter over Wi-Fi/Thread, Home Assistant, MQTT).

### Intercom Core (`intercom-core`)
The central, hardware-agnostic and protocol-agnostic domain and application orchestrator. Coordinates door unlocking, ring detection, domain event reporting, auto-relock timeouts, and factory reset routines. Zero dependencies on microcontroller peripherals, physical LEDs, or network stacks.

### Application Services / Use Cases
Autonomous async services orchestrating domain rules and port interfaces:
- **AccessController**: Deep application use case coordinating entrance door unlock actuation, auto-relock safety deadlines, and visitor ring announcements.
- **RingWatcher**: Monitors physical ring triggers and dispatches them to the core domain via `AccessControllerHandle`.
- **ResetService**: Coordinates credential erasure and reboot sequence upon maintenance button trigger.

### Lifecycle Watch
The continuous state broadcast (`Watch`) providing zero-drop, zero-allocation system lifecycle updates to observers without queue limits.

## Ports (Seams)

### Inbound Ports
Primary driving contracts allowing external smart home adapters and hardware triggers to drive the core:
- **AccessControllerHandle**: The idiomatic command handle wrapping a typed command channel (`AccessCommand`), enabling external adapters to trigger unlocks and rings with value semantics.

### Outbound Ports
Abstract traits defined by `intercom-core` representing required capabilities:
- **IntercomReporter**: Reports visitor chime/ring activity and lock latch state (`LockState`) to external ecosystems and local feedback adapters (protocol-agnostic, zero Matter or hardware terms).
- **DoorDriver**: Triggers physical entrance door unlock actuation.
- **RingDriver**: Blocks until a debounced hardware ring event occurs.
- **ResetTriggerDriver**: Waits for maintenance button hold triggers.
- **SystemResetDriver**: Erases persistent credentials and triggers hardware reboot.

## Adapters (Edge Implementations)

### Intercom Hardware Adapter (`intercom-adapter-gpio`)
Concrete hardware drivers binding port traits to physical intercom lines and ESP32-C6 GPIO pins:
- `GpioRingDriver` (optocoupler sensing ring voltage, e.g. IR204E AC/DC buzzer line)
- `GpioDoorDriver` (PhotoMOS relay closing intercom door strike line)
- `GpioLedDriver` (active-low status LED; implements `IntercomReporter` for access blinks and observes `SystemStateReceiver` for lifecycle patterns)
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
