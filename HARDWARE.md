# Hardware Documentation (`Intercom-HEAD` Rev 1.1)

Hardware reference and schematic documentation for the **Smart Intercom** custom PCB (`Intercom-HEAD` Rev 1.1, drawn 01-2026).

---

## Overview

The **Intercom-HEAD Rev 1.1** board is a custom SMD PCB built around a bare **`ESP32-C6-WROOM-1-N8`** (8 MB flash) module. It connects to an existing apartment intercom station (such as the TekTone IR204E) via a 4-pin connector (`U9`), providing optically isolated ring detection and solid-state door strike actuation with native USB-C power and programming.

---

## Bill of Materials (BOM)

### Active & Integrated Components

| Ref | Component | Part Number | Package | Qty | Purpose |
|-----|-----------|-------------|---------|-----|---------|
| `ESP-32-C6` | Wireless MCU Module | `ESP32-C6-WROOM-1-N8` | SMD-29 (18×25.5 mm) | 1 | RISC-V MCU (Wi-Fi 6 + BLE 5, 8 MB Flash) |
| `U4` | 3.3V LDO Regulator | `AP2112K-3.3TRG1` | SOT-23-5 | 1 | 600 mA low-dropout 5V → 3.3V regulator |
| `U7` | PhotoMOS Solid-State Relay | `AQY212SX` | SOP-4 | 1 | Optically isolated door button contact closure |
| `U8` | Phototransistor Optocoupler | `HCPL-817(AT)` | SMD-4 | 1 | Optically isolated ring signal detection |
| `D1` | Bridge Rectifier | `MB6S` | SOIC-4 | 1 | Full-wave AC/DC rectification for ring line |
| `LED` | Status Indicator LED | `NCD0805R1` | 0805 | 1 | Active-low red status LED on `GPIO19` |

### Passive Components

| Ref | Value | Package | Qty | Circuit / Purpose |
|-----|-------|---------|-----|-------------------|
| `R1`, `R2` | `5.1kΩ` | 0805/0603 | 2 | USB-C `CC1` / `CC2` sink pull-down resistors |
| `R3` | `2kΩ` | 0805/0603 | 1 | Status LED current-limiting resistor (`+3V3` → `LED` anode) |
| `R4` | `10kΩ` | 0805/0603 | 1 | `EN` (Reset) pull-up resistor to `+3V3` |
| `R5` | `10kΩ` | 0805/0603 | 1 | `GPIO8` boot strapping pull-up resistor to `+3V3` |
| `R7` | `680Ω` | 0805/0603 | 1 | `AQY212SX` PhotoMOS LED series current-limiting resistor |
| `R8` | `6.8kΩ` | 0805/0603 | 1 | `HCPL-817(AT)` optocoupler LED series resistor (from `MB6S` `+`) |
| `C1`, `C2` | `2.2µF` | 0805/0603 | 2 | `U4` (`AP2112K-3.3`) input (`+5V`) and output (`+3V3`) decoupling |
| `C3` | `100nF` | 0805/0603 | 1 | `BOOT` button (`GPIO9`) hardware debounce capacitor |
| `C4` | `100nF` | 0805/0603 | 1 | `RESET` button (`EN`) capacitor |
| `C5` | `1µF` | 0805/0603 | 1 | `EN` power-on reset RC delay capacitor |
| `C6` | `22µF` | 0805/1206 | 1 | `ESP32-C6-WROOM-1` bulk supply decoupling (`+3V3`) |
| `C7` | `100nF` | 0805/0603 | 1 | `ESP32-C6-WROOM-1` high-frequency bypass capacitor (`+3V3`) |

### Connectors & Switches

| Ref | Component | Part Number | Qty | Purpose |
|-----|-----------|-------------|-----|---------|
| `USB_C` | USB Type-C Receptacle | `KH-TYPE-C-16P` | 1 | 5V power supply + native USB Serial/JTAG flashing |
| `U9` | 4-Pin Intercom Header | `SM04B-SRSS-TB(LF)(SN)` | 1 | Intercom bus interface (`DOOR1`, `DOOR2`, `RING1`, `RING2`) |
| `BOOT` | Tactile Switch | `TS-1088-AR02016` | 1 | Bootloader mode & 5s/10s system reset button (`GPIO9`) |
| `RESET` | Tactile Switch | `TS-1088-AR02016` | 1 | Hardware chip reset (`EN`) |

---

## Module Pad & GPIO Assignment (`ESP32-C6-WROOM-1-N8`)

| Module Pad | Pin Name | Net Name | Direction | Function |
|-----------:|----------|----------|-----------|----------|
| 1, 28, 29 | `GND` / `EPAD` | `GND` | Power | Local electronics ground |
| 2 | `3V3` | `+3V3` | Power | 3.3V regulated supply (`C6` 22µF + `C7` 100nF) |
| 3 | `EN` | `EN` | Input | Chip enable / reset (`R4` 10kΩ pull-up, `C5` 1µF, `RESET` switch) |
| 4–9 | `GPIO4`–`GPIO7`, `GPIO0`–`GPIO1` | — | Unused | Unconnected (`NC`) |
| 10 | `GPIO8` | `IO8` | Strap | Boot strapping pin (`R5` 10kΩ pull-up to `+3V3`) |
| 11 | `GPIO10` | `IO10` | Unused | Net label present, unused |
| 12 | `GPIO11` | `IO11` | Unused | Net label present, unused |
| 13 | `GPIO12/USB_D-` | `USB_D-` | Bidir | Native USB Serial/JTAG `D-` (`USB_C` pins `A7`/`B7`) |
| 14 | `GPIO13/USB_D+` | `USB_D+` | Bidir | Native USB Serial/JTAG `D+` (`USB_C` pins `A6`/`B6`) |
| 15 | `GPIO9` | `IO_BOOT` | Input | `BOOT` button (`TS-1088-AR02016` + `C3` 100nF to `GND`, active-low) |
| 16 | `GPIO18` | `IO_DOOR` | Output | Door unlock relay trigger (`R7` 680Ω → `U7` `AQY212SX`, active-high) |
| 17 | `GPIO19` | `IO19` | Output | Status LED sink (`NCD0805R1` + `R3` 2kΩ to `+3V3`, active-low) |
| 18–23 | `GPIO20`–`GPIO23`, `NC`, `GPIO15` | — | Unused | Unconnected (`NC`) |
| 24 | `U0RXD/GPIO17` | `RX` | Input | UART0 RX net |
| 25 | `U0TXD/GPIO16` | `TX` | Output | UART0 TX net |
| 26 | `GPIO3` | — | Unused | Unconnected (`NC`) |
| 27 | `GPIO2` | `IO_RING` | Input | Ring optocoupler collector input (`U8` `HCPL-817(AT)`, active-low, internal pull-up) |

---

## Functional Circuit Blocks

### 1. USB-C Power & Native JTAG/Serial (`USB_C` + `U4`)

```text
USB_C (KH-TYPE-C-16P)
 ├── VBUS (A4/B9, B4/A9) ──► +5V ──┬── C1 (2.2µF) ── GND
 │                                 └── U4 (AP2112K-3.3TRG1 VIN & EN)
 │                                      └── VOUT ──► +3V3 ── C2 (2.2µF) ── GND
 ├── CC1 (A5) ── R1 (5.1kΩ) ── GND
 ├── CC2 (B5) ── R2 (5.1kΩ) ── GND
 ├── DP1/DP2 (A6/B6) ────────► USB_D+ (GPIO13, Pad 14)
 ├── DN1/DN2 (A7/B7) ────────► USB_D- (GPIO12, Pad 13)
 └── GND / Shield ───────────► GND
```

- **USB 2.0 Sink Configuration:** `R1` and `R2` (`5.1kΩ`) on `CC1` and `CC2` request standard 5V power from USB-C hosts and power adapters.
- **Native USB Flashing & Logging:** `USB_D+` (`GPIO13`) and `USB_D-` (`GPIO12`) connect directly to the ESP32-C6 internal USB Serial/JTAG controller—no external USB-UART bridge IC is needed.
- **3.3V Regulation:** `U4` (`AP2112K-3.3TRG1`) regulates `+5V` down to `+3V3` (up to 600 mA), with local `2.2µF` input/output ceramic capacitors (`C1`, `C2`) and `22µF` + `100nF` (`C6`, `C7`) directly at the module's `3V3` pin.

---

### 2. Ring Detection Circuit (`INTERCOM-RING`)

Optically isolates AC or DC intercom call signals (e.g., TekTone IR204E terminal `X` and `1`) from the ESP32-C6 logic domain.

```text
U9 Header                              MB6S (D1)              HCPL-817(AT) (U8)          ESP32-C6
─────────                              ─────────              ─────────────────          ────────
Pin 3 (RING1) ───────────────────────► Pin 2 (~)
                                       Pin 1 (+) ── R8 (6.8kΩ) ──► Pin 1 (Anode)
Pin 4 (RING2) ───────────────────────► Pin 4 (~)                   Pin 4 (Collector) ──► IO_RING (GPIO2, Pad 27)
                                       Pin 3 (-) ────────────────► Pin 2 (Cathode)       (Internal Pull-Up)
                                                                   Pin 3 (Emitter)   ──► GND
```

**Operation:**
1. An incoming AC or DC ring signal on `RING1`/`RING2` is full-wave rectified by `D1` (`MB6S`).
2. The rectified current passes through `R8` (`6.8kΩ`) into the infrared LED of `U8` (`HCPL-817(AT)`).
3. `U8`'s phototransistor saturates and pulls `IO_RING` (`GPIO2`) to `GND` (active-low).
4. `GpioRingDriver` configures `GPIO2` with its internal pull-up resistor, detects the falling edge, debounces (`30 ms`), and applies a cooldown window (`2500 ms`).

---

### 3. Door Unlock Circuit (`INTERCOM-DOOR`)

Acts as an optically isolated solid-state switch wired in parallel with the intercom's physical door-release button.

```text
ESP32-C6                               AQY212SX (U7)              U9 Header
────────                               ─────────────              ─────────
IO_DOOR (GPIO18, Pad 16) ── R7 (680Ω) ──► Pin 1 (Anode)           Pin 4 (Load) ──► Pin 1 (DOOR1)
                            GND       ──► Pin 2 (Cathode)         Pin 3 (Load) ──► Pin 2 (DOOR2)
```

**Operation:**
1. To unlock the door, `GpioDoorDriver` drives `IO_DOOR` (`GPIO18`) `HIGH` (`3.3V`).
2. Current (`~3 mA`) flows through `R7` (`680Ω`) into the `AQY212SX` input LED.
3. The `AQY212SX` back-to-back internal MOSFETs turn on bidirectionally, bridging `DOOR1` and `DOOR2` across `U9` pins 1 and 2.
4. After the configured unlock duration (default `5000 ms`, configurable `1–10 s` via Matter / Web Dashboard), `GPIO18` returns `LOW` and the relay opens.

---

### 4. Status LED, Boot & Reset Controls (`LED`, `BOOT`, `RESET`)

- **Status LED (`IO19` / `GPIO19`, Pad 17):**
  - Wired active-low: `+3V3` → `R3` (`2kΩ`) → `LED` (`NCD0805R1`) → `GPIO19`.
  - Driven by `GpioLedDriver` to display boot, Wi-Fi/Matter commissioning, ring, unlock, OTA, and reset hold states.
- **BOOT / Reset Button (`IO_BOOT` / `GPIO9`, Pad 15):**
  - `TS-1088-AR02016` switch with `C3` (`100nF`) hardware debounce capacitor to `GND`.
  - Holding for **5 seconds** triggers an **Ecosystem Reset** (clears Matter fabrics, keeps Wi-Fi).
  - Holding for **10 seconds** triggers a **Factory Reset** (wipes both Matter fabrics and Wi-Fi credentials).
  - Holding `BOOT` while tapping `RESET` enters the ESP32-C6 ROM USB/UART bootloader.
- **Hardware Reset (`EN`, Pad 3):**
  - `R4` (`10kΩ` to `+3V3`) + `C5` (`1µF` to `GND`) provides power-on reset delay; pressing `RESET` (`TS-1088-AR02016` + `C4` `100nF`) pulls `EN` to `GND`.

---

## Known Hardware Issues & Errata (Rev 1.1)

### HW-01: Intermittent Cold Boot Failure Caused by C3 (100nF) on GPIO9

- **Symptoms:** On initial cold power-up (plugging in power), the device occasionally fails to start: the status LED stays off, no Wi-Fi starts, and the device is unresponsive. Quick power-cycling or tapping `RESET` recovers normal operation.
- **Root Cause:**
  - `GPIO9` is the primary boot strapping pin for the ESP32-C6 (`HIGH` = Normal SPI Flash Boot, `LOW` = ROM Download Boot).
  - `GPIO9` relies on the ESP32-C6 internal weak pull-up resistor (~45kΩ).
  - Capacitor `C3` (`100nF`) was placed in parallel with the `BOOT` switch to `GND` for hardware debounce.
  - On cold boot, `C3` is completely discharged (0V). The charging time constant ($\tau = 45\text{k}\Omega \times 100\text{nF} = 4.5\text{ms}$) requires ~6.2ms to reach logic-high threshold ($V_{IH} \approx 2.48\text{V}$).
  - Because `EN` releases reset within 1–2ms, the chip samples `GPIO9` while it is still held LOW by `C3`, tricking the chip into entering ROM Download Boot (waiting for UART flashing) instead of executing firmware.
- **Remediation for Rev 1.1 boards:**
  - Remove / desolder `C3` (`100nF`). Hardware debouncing is redundant because `intercom-adapter-gpio::boot_button` filters debounce in software and reset triggers require holding the button for 5s/10s.
  - Optional rework: solder a `4.7kΩ`–`10kΩ` pull-up resistor between `GPIO9` and `+3V3`.
- **Fix for Rev 2.0 PCB:**
  - Remove `C3` footprint (or mark DNP).
  - Add an external `10kΩ` pull-up resistor from `GPIO9` (Pad 15) to `+3V3`.

### HW-02: Native USB Auto-Flashing Reset Delay on C5 (1µF)

- **Symptoms:** Flashing firmware via native USB-C (`espflash` or `cargo run`) sometimes fails to enter download mode automatically, requiring the developer to hold `BOOT` and tap `RESET`.
- **Root Cause:**
  - The ESP32-C6 internal USB Serial/JTAG controller automatically controls `EN` (Reset) and strapping pins during USB CDC-ACM control requests (DTR/RTS toggling).
  - A $1\mu\text{F}$ capacitor on `EN` (`C5`) makes the reset pulse too slow to discharge and recover within `espflash`'s auto-reset timing window.
- **Fix for Rev 2.0 PCB:**
  - Change `C5` from $1\mu\text{F}$ to **$100\text{nF}$** (`C14663`, JLCPCB Basic Part).
  - This allows the native USB controller to reset the chip into ROM download mode in microseconds without ever pressing physical buttons.

---

## Connector Pinout (`U9` — `SM04B-SRSS-TB`)

| Pin | Net | Function |
|----:|-----|----------|
| 1 | `DOOR1` | PhotoMOS relay contact A (`U7` Pin 4) — connect to intercom Door button side A |
| 2 | `DOOR2` | PhotoMOS relay contact B (`U7` Pin 3) — connect to intercom Door button side B |
| 3 | `RING1` | Bridge rectifier AC input 1 (`D1` Pin 2) — connect to intercom Call/Ring wire (`X`) |
| 4 | `RING2` | Bridge rectifier AC input 2 (`D1` Pin 4) — connect to intercom Common wire (`1`) |
