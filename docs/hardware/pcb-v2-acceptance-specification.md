# PCB v2 (`Intercom-HEAD` Rev 2.0) Hardware Acceptance Specification

> **Specification Type:** Black-Box Product Requirements & Acceptance Criteria  
> **System Purpose:** Smart Intercom Bridge (Analog Multi-Tenant Intercom $\leftrightarrow$ Matter-over-Wi-Fi with Isolated 2-Way Audio)  
> **Evaluation Model:** **Black Box** — Tested strictly at external physical interfaces and the ESP32-C6 microcontroller boundary. Zero internal implementation or discrete component dependencies.

---

## 1. High-Level System Goal

The **`Intercom-HEAD` Rev 2.0** hardware is a dedicated bridge board designed to modernize legacy 4-wire analog building intercom stations (such as the TekTone IR204E system) into a native **Matter-over-Wi-Fi** smart home device. 

The hardware assembly must operate as an autonomous, self-contained black box that:
1. **Safely interfaces with multi-tenant building intercom wiring** without injecting ground loops, DC noise, or electrical hazards into the shared building riser.
2. **Detects incoming ring/buzzer events** across high-voltage AC or DC call lines.
3. **Actuates building door strike mechanisms** remotely on command from smart home ecosystems (Apple Home, Home Assistant, Google Home).
4. **Enables isolated, full-duplex two-way audio communication** between the building lobby panel and the smart home network.
5. **Provides native USB flashing and configuration** with universal cable reversibility.
6. **Maintains robust Wi-Fi 6 wireless performance** in residential RF environments.

```
                   ┌─────────────────────────────────────────────────────────────┐
                   │             PCB v2 BLACK BOX IMPLEMENTATION                 │
                   │                                                             │
BUILDING RISER     │  ┌─────────────────────────┐     ┌───────────────────────┐  │  SMART HOME NETWORK
4-Wire Terminal    │  │                         │     │                       │  │  (Matter / Thread / Wi-Fi)
├── BUS_2 (Talk)   │──┤                         │     │  ESP32-C6 Controller  │  │
├── BUS_3 (Listen) │──┤   Galvanic Isolation    │     │                       │  │
├── BUS_X (Ring)   │──┤            &            │◄───►│  * Firmware Logic     ├──┼──► Wi-Fi 6 (802.11ax)
└── BUS_COM        │──┤   Audio Conditioning    │     │  * Matter Protocol    │  │  ├── Door Lock Cluster
                   │  │                         │     │  * Audio Streaming    │  │  └── Occupancy Cluster
                   │  └─────────────────────────┘     └───────────────────────┘  │
                   │               ▲                              ▲              │
                   │               └──────────────┬───────────────┘              │
                   │                              │                              │
                   │                     [USB-C Receptacle]                      │
                   │                     ├── 5V DC Power Input                   │
                   │                     └── Reversible Flashing / Logs          │
                   └─────────────────────────────────────────────────────────────┘
```

---

## 2. External System Interfaces

### 2.1 Interface 1: Building Intercom Riser (Terminal J1)
* **Physical Form:** 4-position industrial screw terminal block.
* **Ground Domain:** `BUS_COM` (Riser Common Return) — completely isolated from local digital ground.

| Terminal Pin | Name | Functional Purpose | Electrical Operating Envelope |
|:---:|:---|:---|:---|
| **Pin 1** | **`BUS_2`** | Talk (Microphone Riser) | Active DC loop current sink ($20\text{–}30\text{ mA}$) + bidirectional voice transmission ($0.5\text{–}2.0\text{ V}_{\text{p-p}}$ audio). Quiescent DC bias: $+12\text{V}$ to $+24\text{V DC}$. |
| **Pin 2** | **`BUS_3`** | Listen (Speaker Riser) | Audio signal from entrance lobby panel ($0.2\text{–}1.5\text{ V}_{\text{RMS}}$ AC). |
| **Pin 3** | **`BUS_X`** | Ring / Call Input | Call buzzer signal from lobby directory button ($12\text{–}24\text{ V AC/DC}$ nominal; up to $90\text{ V}$ peak transient spike). |
| **Pin 4** | **`BUS_COM`**| Riser Common Return | Reference ground for building wiring. |

---

### 2.2 Interface 2: Power & Diagnostic Interface (USB Type-C)
* **Physical Form:** Universal USB Type-C receptacle.
* **Ground Domain:** `ESP_GND` (Clean local digital ground).
* **Power Requirements:** $+5.0\text{ V DC} \pm 5\%$, capable of supplying up to $1.0\text{ A}$ peak during RF bursts.
* **Cable Reversibility:** Plug-and-play operation in **both** cable insertion orientations (Side A upright, Side B upside-down).
* **Configuration:** Standard Downstream Facing Port (UFP) signaling via independent $5.1\text{ k}\Omega$ pull-down resistors on CC1 and CC2.

---

### 2.3 Interface 3: User Controls & Visual Indicator
* **Status Indicator:** Single visible indicator on `GPIO19` (Active LOW: illuminated when driven LOW).
* **Hardware Reset:** Tactile button to force hardware restart of the controller.
* **Bootloader Switch:** Tactile button to force controller into ROM download mode when held during reset.

---

### 2.4 Interface 4: Wireless / RF Domain
* **Operating Frequencies:** $2.4\text{ GHz}$ ISM band supporting:
  - Wi-Fi 6 (IEEE 802.11b/g/n/ax)
  - Bluetooth 5.3 Low Energy (Matter commissioning)
  - IEEE 802.15.4 / Thread
* **Antenna Boundary:** Minimum $15\text{ mm}$ clear perimeter free of metal, ground planes, or conductive shields.

---

## 3. ESP32-C6 Microcontroller Interface Contract

The black box provides the following standardized GPIO interface to the ESP32-C6 microcontroller:

| Signal Name | ESP32 Pin | GPIO # | Direction | Active Logic | Behavioral Contract |
|:---|:---:|:---:|:---:|:---:|:---|
| **`IO_RING`** | Pad 27 | `GPIO2` | Input to MCU | Active LOW | Pulled down to logic LOW when an incoming call/buzz is detected on `BUS_X`. Must be assigned to **`LP_GPIO2`** for Deep-Sleep wake compatibility. |
| **`IO_DOOR`** | Pad 16 | `GPIO18`| Output from MCU | Active HIGH | Driving HIGH engages electronic door strike across `BUS_2` and `BUS_3`. Must default to open-circuit during MCU boot. |
| **`IO_LISTEN`** | Pad 7 | `GPIO7` | Output from MCU | Active HIGH | Driving HIGH connects the building listen line (`BUS_3`) to the audio input path. |
| **`IO_TALK`** | Pad 11 | `GPIO10`| Output from MCU | Active HIGH | Driving HIGH sinks DC talk current on `BUS_2` and connects the audio output path. |
| **`I2C_SDA`** | Pad 8 | `GPIO0` | Bidirectional | Open-Drain | Audio subsystem digital control bus (with hardware pull-up to $3.3\text{ V}$). |
| **`I2C_SCL`** | Pad 9 | `GPIO1` | Output from MCU | Open-Drain | Audio subsystem clock bus (with hardware pull-up to $3.3\text{ V}$). |
| **`I2S_TX`** | Pad 18 | `GPIO20`| Output from MCU | CMOS | Serial digital audio stream to speaker/lobby. |
| **`I2S_RX`** | Pad 19 | `GPIO21`| Input to MCU | CMOS | Serial digital audio stream from lobby microphone. |
| **`I2S_MCLK`** | Pad 20 | `GPIO22`| Output from MCU | CMOS | Audio subsystem master clock ($12.288\text{ MHz}$). |
| **`I2S_BCLK`** | Pad 21 | `GPIO23`| Output from MCU | CMOS | Audio bit clock ($3.072\text{ MHz}$). |
| **`I2S_WCLK`** | Pad 23 | `GPIO15`| Output from MCU | CMOS | Audio word/channel select clock ($16\text{ kHz}$). |
| **`CODEC_RST_N`**| Pad 12 | `GPIO11`| Output from MCU | Active HIGH | Keeps audio subsystem in reset until initialized by firmware. Defaults LOW. |
| **`USB_D+` / `D-`**| Pads 14/13 | `GPIO13/12`| Bidirectional | Differential | Reversible USB 2.0 Full Speed data lines. |
| **`IO_BOOT`** | Pad 15 | `GPIO9` | Input to MCU | Active LOW | Boot strapping pin: Pulled HIGH to 3.3V. **Must have zero capacitance to ground.** |
| **`IO8`** | Pad 10 | `GPIO8` | Input to MCU | Active HIGH | Boot strapping pin: Pulled HIGH to 3.3V. |

---

## 4. Black-Box Acceptance Criteria & Test Plan

```
                   ┌────────────────────────────────────────────────────────┐
                   │           ACCEPTANCE VERIFICATION SUITE                │
                   └────────────────────────────────────────────────────────┘
                                               │
      ┌────────────────┬───────────────────────┼────────────────────────┬────────────────┐
      ▼                ▼                       ▼                        ▼                ▼
[AC-01: Power]  [AC-02: Safety]        [AC-03: Ring]            [AC-04: Strike]   [AC-05: Audio]
• 5V USB In     • 1.5kV Isolation      • 12-24V AC/DC In        • 300mA Load      • Listen SNR >70dB
• Stable Rails  • 2.5mm Creepage       • Pulse on GPIO2         • <1.5Ω On-Res    • Talk DC Loop 20-30mA
• <1A Peak      • Zero DC Leakage      • Deep-Sleep Wake        • Zero Chatter    • Transformer Isolated
                                               │
                               ┌───────────────┴────────────────┐
                               ▼                                ▼
                        [AC-06: USB Data]                [AC-07: RF/Wi-Fi]
                        • Reversible Plug                • Wi-Fi 6 RSSI > -65dBm
                        • Auto-Flash Console             • BLE Pairing < 3m
                        • JTAG Debug                     • Zero Antenna Detuning
```

---

### AC-01: Power Delivery & Voltage Regulation
* **Test Method:** Power the assembly via standard USB Type-C power supply ($5.0\text{ V DC}$).
* **Acceptance Criteria:**
  1. Unit powers on successfully from any standard USB-C host or USB Power Delivery charger without negotiation failure.
  2. Operating current in idle state is $< 100\text{ mA}$.
  3. Peak current during Wi-Fi transmission bursts remains $< 800\text{ mA}$.
  4. All internal operating voltages ramp monotonically from $0\text{ V}$ to steady-state within $< 10\text{ ms}$ without oscillations or voltage droop exceeding $5\%$.

---

### AC-02: Galvanic Isolation & High-Voltage Protection
* **Test Method:** Measure DC resistance and apply high-voltage test potential between building riser common (`BUS_COM`) and USB ground (`ESP_GND`).
* **Acceptance Criteria:**
  1. **DC Isolation:** Measured resistance across the barrier is **$\ge 100\text{ M}\Omega$** (open-circuit).
  2. **Dielectric Withstand:** The barrier withstands **$1500\text{ V}_{\text{RMS}}$ ($60\text{ Hz}$ for $60\text{ seconds}$)** with leakage current $< 10\mu\text{A}$.
  3. **Physical Creepage:** Minimum physical clearance across the isolation moat on all layers is **$\ge 2.5\text{ mm}$**.
  4. **Transient Immunity:** Applying a $90\text{ V}_{\text{peak}}$ AC surge across `BUS_X` and `BUS_COM` results in zero electrical leakage or damage to the microcontroller domain.

---

### AC-03: Ring / Buzzer Signal Detection
* **Test Method:** Apply valid intercom call signals across `BUS_X` and `BUS_COM` while monitoring `IO_RING` (`GPIO2`).
* **Acceptance Criteria:**
  1. An AC call voltage ($12\text{–}24\text{ V}_{\text{RMS}}$, $50/60\text{ Hz}$) causes `IO_RING` to transition LOW within $< 20\text{ ms}$.
  2. A DC buzzer voltage ($+12\text{–}24\text{ V DC}$) causes `IO_RING` to transition LOW within $< 10\text{ ms}$.
  3. Quiescent or floating voltage on `BUS_X` holds `IO_RING` solidly HIGH ($3.3\text{ V}$) without false triggers.
  4. Asserting a ring signal successfully awakens the ESP32-C6 from Deep-Sleep mode via `LP_GPIO2`.

---

### AC-04: Door Strike Actuation
* **Test Method:** Connect a simulated intercom door strike load ($24\text{ V DC}$, $100\ \Omega$ load, representing $240\text{ mA}$ strike current) between `BUS_2` and `BUS_3`. Assert `IO_DOOR` (`GPIO18`) HIGH.
* **Acceptance Criteria:**
  1. Driving `IO_DOOR` HIGH causes the circuit between `BUS_2` and `BUS_3` to close within $< 1.0\text{ ms}$.
  2. The dynamic on-resistance between `BUS_2` and `BUS_3` while active is **$< 1.5\ \Omega$** (voltage drop across switch $< 0.4\text{ V}$ at $250\text{ mA}$).
  3. Driving `IO_DOOR` LOW opens the circuit within $< 1.0\text{ ms}$ with leakage current $< 1\mu\text{A}$ at $30\text{ V DC}$.
  4. **Boot Safety:** During cold-boot, hardware reset, or firmware crash, the door strike circuit remains guaranteed OPEN. Zero relay chatter or unintended clicks during power transitions.

---

### AC-05: Two-Way Audio — Listen Channel (Lobby $\rightarrow$ MCU)
* **Test Method:** Drive `IO_LISTEN` (`GPIO7`) HIGH. Inject a calibrated audio test tone ($1\text{ kHz}$ sine wave, $500\text{ mV}_{\text{RMS}}$) across `BUS_3` and `BUS_COM`. Stream digitized audio from the ESP32-C6 over I2S.
* **Acceptance Criteria:**
  1. Passband frequency response covers **$300\text{ Hz}$ to $3.4\text{ kHz}$** within $\pm 2.0\text{ dB}$ (telephony voice band).
  2. Total Harmonic Distortion plus Noise (THD+N) is **$< 0.5\%$** at $1\text{ kHz}$.
  3. Signal-to-Noise Ratio (SNR) of digitized audio is **$\ge 70\text{ dB}$**.
  4. High-frequency digital hash, Wi-Fi RF clicks, and $60\text{ Hz}$ AC line hum are below audible threshold ($< -65\text{ dBFS}$).

---

### AC-06: Two-Way Audio — Talk Channel (MCU $\rightarrow$ Lobby)
* **Test Method:** Drive `IO_TALK` (`GPIO10`) HIGH with $+18\text{ V DC}$ applied to `BUS_2` referenced to `BUS_COM`. Stream a $1\text{ kHz}$ audio tone from the ESP32-C6 over I2S to the audio subsystem.
* **Acceptance Criteria:**
  1. **DC Loop Current:** Sinks **$20\text{ mA}$ to $30\text{ mA}$** of steady DC loop current from `BUS_2` to power and bias the building intercom amplifier.
  2. **Power Dissipation:** The talk circuit handles the DC loop current continuously for $5\text{ minutes}$ without thermal degradation or component overheating ($T_{\text{rise}} < 30^\circ\text{C}$).
  3. **Audio Injection:** Injects up to **$1.0\text{ V}_{\text{RMS}}$ AC audio** onto `BUS_2` without clipping or saturation.
  4. **DC Isolation:** Zero DC current leaks into the digital audio subsystem (complete galvanic DC blocking).

---

### AC-07: USB Type-C Connectivity & Auto-Flashing
* **Test Method:** Connect the assembly to a host computer running `cargo run` and serial monitoring tools. Test in both cable insertion orientations.
* **Acceptance Criteria:**
  1. **Orientation A (Upright):** Enumerates as `Espressif USB JTAG/serial debug unit` on `/dev/cu.usbmodem*`. Flashes firmware cleanly at default baud rate.
  2. **Orientation B (Inverted $180^\circ$):** Enumerates identically as `Espressif USB JTAG/serial debug unit`. Flashes firmware cleanly with zero dropped packets.
  3. **Serial Terminal:** Captures continuous real-time diagnostic output from `intercom-firmware` without character corruption.

---

### AC-08: Wireless RF Range & Matter Commissioning
* **Test Method:** Flash production `intercom-firmware` binary. Place unit inside a standard plastic electrical enclosure $10\text{ meters}$ away from an 802.11ax Wi-Fi access point through one residential drywall partition.
* **Acceptance Criteria:**
  1. **Wi-Fi RSSI:** Measured signal strength at the device is **$\ge -65\text{ dBm}$**.
  2. **Bluetooth LE Advertising:** Discovered by iOS / Apple Home app within $< 3\text{ seconds}$ at a distance of $3\text{ meters}$.
  3. **Matter Commissioning:** Device pairs to Apple Home / Home Assistant on first attempt, generating Door Lock and Occupancy Sensor endpoints.
  4. **Antenna Keep-Out:** Measured RF antenna return loss ($S_{11}$) remains below $-10\text{ dB}$ across $2400\text{–}2484\text{ MHz}$ (no ground plane detuning).

---

### AC-09: Cold-Boot Reliability & Fail-Safe Operation
* **Test Method:** Execute 50 consecutive power-cycling cycles (disconnecting and reconnecting 5V USB power at random intervals between $100\text{ ms}$ and $10\text{ s}$).
* **Acceptance Criteria:**
  1. Firmware boots cleanly and joins Wi-Fi network **50 out of 50 times** ($100\%$ reliability).
  2. Zero instances of ROM download boot lockup or cold-boot deadlock.
  3. Relays remain completely de-energized during the power-on and boot phases.
