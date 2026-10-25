# Intercom-HEAD Rev 2.0 (Audio Support) — Text Schematic

Complete block-by-block text schematic for **Intercom-HEAD Rev 2.0**, extending **`Intercom-HEAD` Rev 1.1** with isolated two-way audio (`TLV320AIC3204` + `2× TY-250P` + solid-state `AQY212SX` switching) and incorporating the `HW-01` cold-boot and `HW-02` USB auto-flash fixes.

All reference designators and LCSC C-IDs match [`intercom-head-rev2-bom.md`](intercom-head-rev2-bom.md).

---

## Ground Domains & Galvanic Isolation Boundary

The board is divided into two strictly isolated ground domains separated by a minimum **2.5 mm all-layer copper keepout slot**:

1. **`ESP_GND` (Local Electronics Ground):**
   - USB-C shield & ground, `ESP32-C6-WROOM-1-N8`, `U4`/`U11`/`U12`/`U13` LDOs, `U10` (`TLV320AIC3204` codec), `T_RX` secondary, `T_TX` primary, `U8` (`HCPL-817`) transistor side (Pins 3–4), and `U7`/`U14`/`U15` (`AQY212SX`) LED side (Pins 1–2).
2. **`BUS_COM` (Building Intercom Bus Common — Terminal `1`):**
   - `U9` connector (`BUS_1`, `BUS_2`, `BUS_3`, `BUS_X`), `D1` (`MB6S`) + `U8` (`HCPL-817`) LED side (Pins 1–2), `U7`/`U14`/`U15` (`AQY212SX`) MOSFET load side (Pins 3–4), `T_RX` primary, `T_TX` secondary, and `R_TALK`.

**Only 5 components cross the isolation boundary:** `U7`, `U8`, `U14`, `U15`, `T_RX`, and `T_TX`.

---

## Block 1: `USB-C` & `LDO_5V` (System Power & Native USB Auto-Flash)

*(Retained from `Intercom-HEAD` Rev 1.1)*

```text
USB_C (KH-TYPE-C-16P, C2765186)
  ├── VBUS (A4/B9, B4/A9) ──┬──► +5V
  │                         ├── C1 (2.2µF, C23630) ──► ESP_GND
  │                         └──► U4 (AP2112K-3.3TRG1, C39734)
  │                               ├── Pin 1 (VIN) & Pin 3 (EN) ◄── +5V
  │                               ├── Pin 2 (GND) ───────────────► ESP_GND
  │                               ├── Pin 4 (NC)  ───────────────► X
  │                               └── Pin 5 (VOUT) ──┬───────────► +3V3
  │                                                  └── C2 (2.2µF, C23630) ──► ESP_GND
  ├── CC1 (A5) ── R1 (5.1kΩ, C23186) ──► ESP_GND
  ├── CC2 (B5) ── R2 (5.1kΩ, C23186) ──► ESP_GND
  ├── DP1/DP2 (A6/B6) ─────────────────► USB_D+ (GPIO13, WROOM Pad 14)
  ├── DN1/DN2 (A7/B7) ─────────────────► USB_D- (GPIO12, WROOM Pad 13)
  └── GND (A1/B12, B1/A12, Shield 1-4) ► ESP_GND
```

---

## Block 2: `CODEC_POWER` (Dedicated Low-Noise Audio LDO Tree)

*(New in Rev 2.0 — uses JLCPCB Basic SOT-23-5 LDOs: `ME6211` pinout: `1=VIN, 2=GND, 3=EN, 4=NC, 5=VOUT`)*

```text
+5V ──┬── C9 (4.7µF, C19666) ──► ESP_GND
      └──► U11 (ME6211C33M5G 3.3V, C82942) [Pin 1=VIN, Pin 3=EN, Pin 2=GND]
            └── Pin 5 (VOUT) ──┬──► +3V3_CODEC (to U10 Pin 26 LDOIN)
                               ├── C10 (4.7µF, C19666) ──► ESP_GND
                               │
                               ├──┬── C11 (4.7µF, C19666) ──► ESP_GND
                               │  └──► U12 (ME6211C18M5G 1.8V, C38064) [Pin 1=VIN, Pin 3=EN, Pin 2=GND]
                               │        └── Pin 5 (VOUT) ──┬──► +1V8_ANALOG (to U10 Pin 24 AVDD)
                               │                           └── C12 (4.7µF, C19666) ──► ESP_GND
                               │
                               └──┬── C13 (4.7µF, C19666) ──► ESP_GND
                                  └──► U13 (ME6211C18M5G 1.8V, C38064) [Pin 1=VIN, Pin 3=EN, Pin 2=GND]
                                        └── Pin 5 (VOUT) ──┬──► +1V8_DIGITAL (to U10 Pin 29 DVDD)
                                                           └── C14 (4.7µF, C19666) ──► ESP_GND
```

---

## Block 3: `BOOT`, `RESET`, & `LED` (With `HW-01` & `HW-02` Fixes)

*(Updated from Rev 1.1: `C3` removed and `R6` 10kΩ added on `IO_BOOT` to fix cold-boot hang; `C5` reduced from 1µF to 100nF for hands-free USB auto-flash)*

```text
[LED Block — Active Low]
+3V3 ── R3 (2kΩ, C4190) ──►|── LED (NCD0805R1, C84256) ──► IO19 (GPIO19, WROOM Pad 17)

[BOOT Block — HW-01 Cold-Boot Fix Applied (NO C3 capacitor!)]
+3V3 ── R6 (10kΩ, C25804) ──┬──► IO_BOOT (GPIO9, WROOM Pad 15)
                            │
                          ┌─┴─┐
                          │   │ BOOT (TS-1088-AR02016, C115358)
                          └─┬─┘
                            │
                         ESP_GND

[RESET Block — HW-02 Auto-Flash Fix Applied (C5 = 100nF)]
+3V3 ── R4 (10kΩ, C25804) ──┬────────────────────────────► EN (WROOM Pad 3)
                            │
                            ├── C5 (100nF, C14663) ──────► ESP_GND
                            ├── C4 (100nF, C14663) ──────► ESP_GND
                          ┌─┴─┐
                          │   │ RESET (TS-1088-AR02016, C115358)
                          └─┬─┘
                            │
                         ESP_GND
```

---

## Block 4: `ESP32-C6-WROOM-1-N8` (Main MCU — Pad-by-Pad Schematic)

```text
                                ┌──────────────────────────────────────────┐
                                │           ESP32-C6-WROOM-1-N8            │
                                │                (C5283181)                │
                                │                                          │
                  ESP_GND ◄─────┤ 1  GND                            GND 28 ├─────► ESP_GND
                                │                                  EPAD 29 ├─────► ESP_GND
+3V3 ──┬── C6 (22µF, C45783) ───┤ 2  3V3                                   │
       └── C7 (100nF, C14663)   │                                          │
                       EN ─────►│ 3  EN                     GPIO2/FSPIQ 27 │◄───── IO_RING
                        X ──────┤ 4  GPIO4                        GPIO3 26 ├────── X
                        X ──────┤ 5  GPIO5                 U0TXD/GPIO16 25 ├─────► TX (Test Pad)
                        X ──────┤ 6  GPIO6                 U0RXD/GPIO17 24 │◄───── RX (Test Pad)
                IO_LISTEN ◄─────┤ 7  GPIO7                       GPIO15 23 ├─────► I2S_WCLK
                CODEC_SDA ◄────►│ 8  GPIO0                           NC 22 ├────── X
                CODEC_SCL ─────►│ 9  GPIO1                       GPIO23 21 ├─────► I2S_BCLK
+3V3 ── R5 (10kΩ, C25804) ──────┤ 10 GPIO8 (STRAP)               GPIO22 20 ├─────► I2S_MCLK
                  IO_TALK ◄─────┤ 11 GPIO10                      GPIO21 19 │◄───── I2S_RX
              CODEC_RST_N ◄─────┤ 12 GPIO11                      GPIO20 18 ├─────► I2S_TX
                   USB_D- ◄────►│ 13 GPIO12/USB_D-               GPIO19 17 ├─────► IO19 (LED)
                   USB_D+ ◄────►│ 14 GPIO13/USB_D+               GPIO18 16 ├─────► IO_DOOR
                                │                                 GPIO9 15 │◄───── IO_BOOT
                                └──────────────────────────────────────────┘
```

---

## Block 5: `AUDIO-CODEC` (`U10` — `TLV320AIC3204IRHBR`, VQFN-32 `C245642`)

```text
I2C Pull-Ups & Reset:
  +3V3 ── R9  (2.2kΩ, C4197)  ──► CODEC_SDA (GPIO0)
  +3V3 ── R10 (2.2kΩ, C4197)  ──► CODEC_SCL (GPIO1)
  CODEC_RST_N (GPIO11) ── R11 (10kΩ, C25804) ──► ESP_GND

I2S Series Damping (33Ω, C25111):
  I2S_MCLK (GPIO22) ── R12 (33Ω) ──► U10 Pin 1  (MCLK)
  I2S_BCLK (GPIO23) ── R13 (33Ω) ──► U10 Pin 2  (BCLK)
  I2S_WCLK (GPIO15) ── R14 (33Ω) ──► U10 Pin 3  (WCLK)
  I2S_TX   (GPIO20) ── R15 (33Ω) ──► U10 Pin 4  (DIN)
  I2S_RX   (GPIO21) ◄─ R16 (33Ω) ─── U10 Pin 5  (DOUT)

                         ┌─────────────────────────────────────────────┐
                         │         U10: TLV320AIC3204IRHBR             │
                         │             (VQFN-32, C245642)              │
                         │                                             │
  I2S_MCLK ── R12 (33Ω) ►│ 1  MCLK                            IN1_L 13 │◄── CODEC_IN1_L (from RX path)
  I2S_BCLK ── R13 (33Ω) ►│ 2  BCLK                            IN1_R 14 ├─── X
  I2S_WCLK ── R14 (33Ω) ►│ 3  WCLK                         IN2_L/R 15,16├── X
  I2S_TX   ── R15 (33Ω) ►│ 4  DIN                          IN3_L/R 20,21├── X
  I2S_RX   ◄─ R16 (33Ω) ─┤ 5  DOUT                          MICBIAS 19 ├─── X
                         │                                             │
         CODEC_SCL ─────►│ 9  SCL/SSZ                           LOL 22 ├──► CODEC_LOL (to TX path)
         CODEC_SDA ◄────►│ 10 SDA/MOSI                          LOR 23 ├─── X
       CODEC_RST_N ─────►│ 31 RESET                         HPL/HPR 25,27├─ X
                         │                                  MISO/SCLK 8,11├─ X
           ESP_GND ─────►│ 12 SPI_SELECT (0 = I2C Mode, Addr 0x18)  32 ├─── X (GPIO/MFP5)
           ESP_GND ─────►│ 30 LDO_SELECT (0 = External 1.8V LDOs)      │
                         │                                             │
  +3V3 ──┬──────────────►│ 6  IOVDD                             REF 18 ├─── C8 (10µF, C15850) ──► ESP_GND
         └─ C15 (100nF)  │                                             │
  +1V8_ANALOG ──┬───────►│ 24 AVDD                             AVSS 17 ├───► ESP_GND
                └─ C16   │                                     DVSS 7,28├──► ESP_GND
  +1V8_DIGITAL ─┬───────►│ 29 DVDD                             EPAD 33 ├───► ESP_GND
                └─ C17   │                                             │
  +3V3_CODEC ───┬───────►│ 26 LDOIN                                    │
                └─ C18   └─────────────────────────────────────────────┘
  (Note: C15, C16, C17, C18 = 100nF C14663 placed directly adjacent to pins 6, 24, 29, 26 to ESP_GND)
```

---

## Block 6: `INTERCOM-RING` & `INTERCOM-DOOR`

*(Retained from `Intercom-HEAD` Rev 1.1, plus `R21` 100kΩ boot safety pull-down on `IO_DOOR`)*

```text
[INTERCOM-RING: BUS_X & BUS_1 ──► IO_RING (GPIO2)]
                                                    ║  ISOLATION
  BUS_X (J1 Pin 3) ──► Pin 2 (~)  D1: MB6S  Pin 1 (+) ── R8 (6.8kΩ, C23220) ──► Pin 1 (Anode)   U8: HCPL-817(AT)  Pin 4 (Coll) ──► IO_RING (GPIO2, Pad 27)
  BUS_1 (J1 Pin 4) ──► Pin 4 (~)  (C8014)   Pin 3 (-) ────────────────────────► Pin 2 (Cathode)     (C4998)        Pin 3 (Emit) ──► ESP_GND
                                                    ║  BARRIER

[INTERCOM-DOOR: IO_DOOR (GPIO18) ──► Short BUS_2 & BUS_3]
                                                    ║  ISOLATION
  IO_DOOR (GPIO18, Pad 16) ──┬── R7 (680Ω, C23214) ─► Pin 1 (Anode)   U7: AQY212SX  Pin 4 (Load) ──► BUS_2 (J1 Pin 1)
                             ├── R21 (100kΩ, C25803)┐                 (C116744)     Pin 3 (Load) ──► BUS_3 (J1 Pin 2)
                             └── ESP_GND ◄──────────┴─ Pin 2 (Cathode)
                                                    ║  BARRIER
```

---

## Block 7: `AUDIO-RX (LISTEN)` (`BUS_3` → `U14` → `T_RX` → `CODEC_IN1_L`)

*(Connects building lobby audio on `BUS_3` to Codec `IN1_L` when `IO_LISTEN` (`GPIO7`) is `HIGH`)*

```text
  [ESP_GND Domain]                                  ║  [BUS_COM Domain]
                                                    ║
  IO_LISTEN (GPIO7, Pad 7)                          ║  BUS_3 (J1 Pin 2)
    ├── R19 (100kΩ, C25803) ──► ESP_GND             ║    │
    └── R17 (680Ω, C23214)  ──► Pin 1  U14: AQY212SX║    ▼
                     ESP_GND ──► Pin 2  (C116744)   ║  Pin 4 (Load)
                                                    ║  Pin 3 (Load) ──► RX_LINE
                                                    ║                     │
                                                    ║                   C_RX1 (1µF/100V, C96942)
                                                    ║                     │
                                                    ║                   R_RX1 (47kΩ 1206, C17937)
                                                    ║                     │
  RX_SEC ◄───────────────────── Sec(Top)   T_RX     ║  Pri(Top) ◄─────────┴──┬── RX_PRI
    │                                     1:1 600Ω  ║                        │
    ├── R_RX3 (1kΩ, C21190) ──► ESP_GND  (C2900762) ║                      R_RX2 (1kΩ, C21190)
    ├── D2 (1N5819WS, C2128): Anode=RX_SEC, Cathode=ESP_GND                  │
    ├── D3 (1N5819WS, C2128): Anode=ESP_GND, Cathode=RX_SEC                  ▼
    │                                               ║  Pri(Bot) ──────────► BUS_COM (BUS_1)
  ESP_GND ◄──────────────────── Sec(Bot)            ║
    │
    └── From RX_SEC:
        RX_SEC ── C_RX2 (1µF, C15849) ── R_RX4 (1kΩ, C21190) ──┬──► CODEC_IN1_L (U10 Pin 13)
                                                               └── C_RX3 (470pF, C1605) ──► ESP_GND
```

---

## Block 8: `AUDIO-TX (TALK)` (`CODEC_LOL` → `T_TX` → `R_TALK` → `U15` → `BUS_2`)

*(Connects Codec `LOL` and the `47Ω` talk termination load `R_TALK` to `BUS_2` when `IO_TALK` (`GPIO10`) is `HIGH`)*

```text
  [ESP_GND Domain]                                  ║  [BUS_COM Domain]
                                                    ║
  CODEC_LOL (U10 Pin 22)                            ║
    │                                               ║
  C_TX1 (10µF, C15850)                              ║
    │                                               ║
  R_TX1 (100Ω, C22775)                              ║
    │                                               ║
    └──► Pri(Top)     T_TX     Sec(Top) ────────────╫──► C_TX2 (22µF/50V, C96943)
                    1:1 600Ω                        ║      │
         Pri(Bot)  (C2900762)  Sec(Bot) ──► BUS_COM ║    R_TX2 (4.7kΩ, C23163)
           │                                (BUS_1) ║      │
           ▼                                        ║      ├──────────────────────► TX_LINE
        ESP_GND                                     ║      │                          │
                                                    ║    R_TALK (47Ω 2W, C61633)      │
                                                    ║      │                          │
                                                    ║      ▼                          ▼
  IO_TALK (GPIO10, Pad 11)                          ║    BUS_COM (BUS_1)            Pin 3 (Load)
    ├── R20 (100kΩ, C25803) ──► ESP_GND             ║                               U15: AQY212SX
    └── R18 (680Ω, C23214)  ──► Pin 1  U15: AQY212SX║                               Pin 4 (Load)
                     ESP_GND ──► Pin 2  (C116744)   ║                                 │
                                                    ║                                 ▼
                                                    ║                               BUS_2 (J1 Pin 1)
```

---

## Block 9: `HEADER` (`J1` — 4-Position Screw Terminal Block)

`J1` (3.5mm or 3.81mm pitch screw terminal, C474898 / C393325) connects the 4 building riser wires directly to the isolated bus nets:

```text
                     ┌──────────────────────────────┐
                     │ J1: 4-Pin Screw Terminal     │
                     │ (3.5mm / 3.81mm, C474898)    │
                     ├──────────────────────────────┤
  BUS_2 (TALK/DOOR1) ┤ 1  ◄──► Terminal 2 (Talk from apartment / Door release A)
  BUS_3 (LSTN/DOOR2) ┤ 2  ◄──► Terminal 3 (Listen from lobby   / Door release B)
  BUS_X (RING1)      ┤ 3  ◄──► Terminal X (Call / Buzzer line)
  BUS_1 (BUS_COM)    ┤ 4  ◄──► Terminal 1 (Intercom Audio/Ring Common = BUS_COM)
                     └──────────────────────────────┘

---

## Operating Truth Table (PhotoMOS States)

| Operating State | `IO_LISTEN` (`GPIO7` / `U14`) | `IO_TALK` (`GPIO10` / `U15`) | `IO_DOOR` (`GPIO18` / `U7`) | Bus Behavior |
|:---|:---:|:---:|:---:|:---|
| **Boot / Idle / Muted** | `LOW (0)` | `LOW (0)` | `LOW (0)` | `BUS_2` & `BUS_3` completely isolated (`R19–R21` pull-downs hold `U7, U14, U15` off); `U8` monitors `BUS_X` for ring |
| **Listen (Visitor → Resident)** | `HIGH (1)` | `LOW (0)` | `LOW (0)` | `U14` closes: `BUS_3` feeds `T_RX` → Codec `IN1_L` → I2S RX |
| **Talk (Resident → Visitor)** | `LOW (0)` | `HIGH (1)` | `LOW (0)` | `U15` closes: applies `R_TALK` (`47Ω`) DC load across `BUS_2`–`BUS_1` and injects Codec `LOL` → `T_TX` onto `BUS_2` |
| **Door Unlock Pulse** | `LOW (0)` | `LOW (0)` | `HIGH (1)` | `U14` & `U15` open first (disconnecting audio), then `U7` shorts `BUS_2` to `BUS_3` |

---

## Circuit Theory of Operation (Human-Language Guide)

### 1. The Big Picture (How the 4-Wire Intercom Works)

In an analog apartment intercom system (such as the TekTone IR204E / PK543A), your station connects to the central basement amplifier via 4 wires:

* **Terminal 1 (`BUS_COM`):** Audio & DC common return.
* **Terminal X (`BUS_X`):** The **Buzzer / Call line**. Carries AC or DC voltage only when someone downstairs presses your buzzer button.
* **Terminal 3 (`BUS_3`):** The **Listen line**. Lobby microphone audio is broadcast into your apartment over this wire.
* **Terminal 2 (`BUS_2`):** The **Talk line**. Transmits your voice back to the front entrance speaker.
* **Shorting Terminals 2 and 3:** Pressing the physical "DOOR" button bridges wire 2 directly to wire 3, which signals the basement amplifier to energize the front door electric strike latch.

---

### 2. Complete Galvanic Isolation (Safety Barrier)

The central rule of this PCB is that **building wires never make direct galvanic electrical contact with your ESP32 or home power**:

```text
 ┌───────────────────────────────────────┐      ║      ┌───────────────────────────────────┐
 │            YOUR APARTMENT             │      ║      │         BUILDING INTERCOM         │
 │                                       │      ║      │                                   │
 │  USB-C (5V) ──► ESP32-C6 (3.3V)       │  AIR ║  AIR │  Building Wiring Riser            │
 │                 TLV320AIC3204 Codec   │  GAP ║  GAP │  (Terminals 1, 2, 3, X)           │
 │                 Wi-Fi / Matter        │      ║      │  Basement Amplifier Panel         │
 │                                       │      ║      │                                   │
 │  Ground: ESP_GND                      │      ║      │  Ground: BUS_COM (Terminal 1)     │
 └───────────────────────────────────────┘      ║      └───────────────────────────────────┘
                                           ISOLATION
                                            BARRIER
```

If a short circuit, 120V mains leak, or lightning surge occurs on the building riser, it stops at the **Isolation Barrier** (`║`). Signals cross this boundary using only two physical media:
1. **Photons (Light):** Inside optocouplers and solid-state PhotoMOS relays (`HCPL-817`, `AQY212SX`).
2. **Magnetic Flux:** Across the cores of 1:1 audio isolation transformers (`TY-250P`).

---

### 3. Step-by-Step Use Cases

#### Use Case 1: Idle (Quiet Standby)
* **User Experience:** Device is connected to Wi-Fi / Matter, waiting in low power.
* **Circuit Operation:**
  * Pull-down resistors (`R19`, `R20`, `R21`) hold all three PhotoMOS switches (`U7`, `U14`, `U15`) completely OFF.
  * Neither Terminal 2 nor Terminal 3 has any load or audio connection. The building riser remains silent.
  * The only active circuit is the Ring detector (`D1` + `U8`), monitoring `BUS_X` and `BUS_1`.

#### Use Case 2: Visitor Rings Your Apartment Buzzer
* **Physical Trigger:** A visitor presses your button downstairs, sending AC/DC voltage across Terminal X and Terminal 1.
* **Circuit Operation:**
  1. The voltage is full-wave rectified by `D1` (`MB6S`).
  2. Resistor `R8` (`6.8kΩ`) limits the current and illuminates the infrared LED of optocoupler `U8` (`HCPL-817`).
  3. The phototransistor inside `U8` turns on, pulling **`GPIO2` (`IO_RING`) to `ESP_GND`**.
  4. The ESP32 debounces the pulse in software and immediately fires a **Doorbell / Motion Notification** across Matter to Apple Home, Home Assistant, and your phone.

#### Use Case 3: You Tap "Listen" on Your Phone (Hear Visitor)
* **User Experience:** You open your app and hear the lobby audio cleanly.
* **Circuit Operation:**
  1. ESP32 drives **`GPIO7` (`IO_LISTEN`) HIGH**.
  2. This illuminates the internal LED of solid-state relay `U14` (`AQY212SX`), silently closing its MOSFET contacts.
  3. Lobby audio enters from **Terminal 3**, passes DC-blocking capacitor `C_RX1`, through input attenuator `R_RX1/R_RX2` (~ -39.6 dB pad to step down building voltages), and into transformer `T_RX`.
  4. Audio crosses the magnetic gap of `T_RX` into `RX_SEC`.
  5. Schottky diodes `D2/D3` clamp loud peaks to ±0.3V so the signal cannot overload or clip the codec.
  6. The signal enters `IN1_L` of `U10` (`TLV320AIC3204`), which digitizes it over I2S at 16 kHz. The ESP32 streams it over Wi-Fi directly to your phone.

#### Use Case 4: You Tap "Talk" on Your Phone (Speak to Visitor)
* **User Experience:** You speak into your phone to talk through the lobby speaker.
* **Circuit Operation:**
  1. ESP32 sets `IO_LISTEN` (`GPIO7`) LOW, waits 30 ms for contacts to settle, and sets **`GPIO10` (`IO_TALK`) HIGH**.
  2. This turns ON solid-state relay `U15` (`AQY212SX`).
  3. `U15` connects **`R_TALK` (47Ω / 2W power resistor)** across Terminal 2 and Terminal 1. This matches the native IR204E speaker impedance and tricks the basement amplifier into switching to talk amplification.
  4. Digital voice packets from your phone are converted by the `TLV320AIC3204` DAC into analog audio on pin `LOL`.
  5. The audio passes through transformer `T_TX`, across the isolation barrier, through `C_TX2` and `R_TX2`, and injects your voice cleanly onto **Terminal 2** to the lobby speaker.

#### Use Case 5: You Tap "Unlock Door"
* **User Experience:** You tap Unlock in Apple Home or the web dashboard; the lobby strike buzzed open and the visitor enters.
* **Circuit Operation:**
  1. The firmware turns OFF both Talk (`U15`) and Listen (`U14`) switches first to prevent click transients.
  2. The ESP32 drives **`GPIO18` (`IO_DOOR`) HIGH**.
  3. PhotoMOS relay `U7` (`AQY212SX`) turns ON, directly **shorting Terminal 2 to Terminal 3** through screw terminal `J1`.
  4. The building amplifier detects the 2-to-3 closure and energizes the entrance door strike latch.
  5. After the configured unlock pulse (e.g. 3–5 seconds), `GPIO18` returns LOW, `U7` opens, and the system returns to secure idle.
