# Intercom-HEAD Rev 2.0 (Audio Support) — EasyEDA / LCSC Bill of Materials

Cost-optimized BOM for **Intercom-HEAD Rev 2.0** with isolated two-way audio support, verified against the live **JLCPCB / LCSC Component Library**.

---

## Cost Optimization Strategy (JLCPCB SMT Assembly)

1. **Maximize JLCPCB Basic Parts:** Basic parts incur **$0.00 feeder setup fees**. All standard 0603/0805 passives, diodes, and tactile switches are mapped to verified Basic Library C-IDs.
2. **Re-use Existing C-IDs:** Reusing components already on `Intercom-HEAD` Rev 1.1 (such as `AQY212SX` for Talk/Listen switching, `680Ω` drive resistors, and `100nF`/`1µF`/`10µF` caps) avoids paying extra Extended Part setup fees ($3.00 per unique line item).
3. **Solid-State PhotoMOS over Mechanical Relays:** Reusing the `AQY212SX` PhotoMOS relay eliminates the need for mechanical relay coils, coil driver transistors (`2N3904`), flyback diodes, and the `SN74LVC125A` buffer IC.
4. **Cold Boot & Auto-Flash Reliability Fixes (HW-01 & HW-02):**
   - **Remove `C3` (100nF) on `GPIO9`** and add an external `10kΩ` pull-up (`R6` Basic `C25804`) to prevent the ESP32-C6 from intermittently entering ROM Download Boot on cold power-up.
   - **Change `C5` on `EN` from `1µF` to `100nF` (`C14663`):** Speeds up the RC reset rise time so the internal USB Serial/JTAG controller can reset the chip cleanly into download mode automatically during `espflash` / `cargo run` without requiring manual button presses.

---

## 1. Baseline Components (Retained from `Intercom-HEAD` Rev 1.1)

| Ref | Value / Description | Package | LCSC Part # | JLCPCB Type | Qty | Est. Unit Price (USD) | Est. Total (USD) | Notes |
|:---|:---|:---|:---|:---|:---:|:---:|:---:|:---|
| `ESP-32-C6` | `ESP32-C6-WROOM-1-N8` | SMD-29 | **C5366877** | Extended | 1 | $4.31 | $4.31 | 8MB Flash RISC-V Module |
| `USB_C` | USB Type-C 16-Pin Receptacle | SMD | **C709357** | Extended | 1 | $0.18 | $0.18 | 5V Power + Native USB Serial/JTAG |
| `U4` | `ME6211C33M5G-N` (500mA LDO) | SOT-23-5 | **C82942** | Extended | 1 | $0.07 | $0.07 | 3.3V System LDO (shares C-ID with U11) |
| `U7` | `AQY212SX` (PhotoMOS Relay) | SOP-4 | **C719754** | Extended | 1 | $0.99 | $0.99 | Door release contact (`GPIO18`) |
| `U8` | `HCPL-817(AT)` | SMD-4 | **C48686289** | Extended | 1 | $0.05 | $0.05 | Ring optocoupler detector (`GPIO2`) |
| `D1` | `MB6S` (0.5A 600V Bridge) | SOIC-4 | **C2886734** | Extended | 1 | $0.06 | $0.06 | Ring AC/DC bridge |
| `LED` | Red LED `NCD0805R1` | 0805 | **C84256** | **Basic** | 1 | $0.014 | $0.014 | Status indicator (`GPIO19`) |
| `BOOT`, `RESET`| Tactile Switch `TS-1088-AR02016` | SMD 4×3mm | **C720477** | **Basic** | 2 | $0.053 | $0.106 | Reset & Boot switches |
| `R1`, `R2` | `5.1kΩ` 1% | 0603 | **C23186** | **Basic** | 2 | $0.002 | $0.004 | USB-C CC pulldowns |
| `R3` | `2kΩ` 1% | 0603 | **C22975** | **Basic** | 1 | $0.002 | $0.002 | LED current limiter |
| `R4`, `R5`, `R6` | `10kΩ` 1% | 0603 | **C25804** | **Basic** | 3 | $0.002 | $0.006 | `EN`, `GPIO8`, and **`GPIO9` pull-up for HW-01 cold boot fix** |
| `R7` | `680Ω` 1% | 0603 (or 0805) | **C23228** (or **C17798**) | **Basic** | 1 | $0.002 | $0.002 | `U7` PhotoMOS drive resistor |
| `R8` | `6.8kΩ` 1% | 0603 | **C23212** | **Basic** | 1 | $0.002 | $0.002 | Ring optocoupler input resistor |
| `C1`, `C2` | `2.2µF` 16V X5R (or 4.7µF `C19666`) | 0603 | **C23630** | **Basic** | 2 | $0.013 | $0.026 | `U4` ME6211 filter caps |
| `C4`, `C5`, `C7`| `100nF` 50V X7R | 0603 | **C14663** | **Basic** | 3 | $0.015 | $0.045 | MCU bypass & `RESET` RC delay (**`C3` removed; `C5` changed from 1µF to 100nF for HW-02 auto-flash**) |
| `C6` | `22µF` 6.3V X5R | 0603 | **C59461** | **Basic** | 1 | $0.031 | $0.031 | MCU 3.3V bulk filter |
| `J1` | 4-Pin 3.5mm/3.81mm Screw Terminal (or `U9` JST-SH `C160404`) | TH / SMD | **C474898** | Extended | 1 | $0.25 | $0.25 | Intercom riser connector (`1, 2, 3, X`) |
| **Subtotal Baseline** | | | | | **23** | | **~$6.15** | |

---

## 2. New Audio Additions (Delta BOM for Rev 2.0)

### A. Codec & Clean Power Tree

| Ref | Value / Description | Package | LCSC Part # | JLCPCB Type | Qty | Est. Unit Price (USD) | Est. Total (USD) | Notes |
|:---|:---|:---|:---|:---|:---:|:---:|:---:|:---|
| `U10` | `TLV320AIC3204IRHBR` | VQFN-32 | **C24109** | Extended | 1 | $1.76 | $1.76 | Low-power audio codec (ADC/DAC) |
| `U11` | `ME6211C33M5G-N` (3.3V LDO) | SOT-23-5 | **C82942** | Extended | 1 | $0.07 | $0.07 | Codec `3V3_CODEC` rail (**Reuses `U4` C-ID: $0 extra feeder fee**) |
| `U12`, `U13` | `ME6211C18M5G-N` (1.8V LDO) | SOT-23-5 | **C236671** | Extended | 2 | $0.07 | $0.14 | Codec `1V8_ANALOG` (`AVDD`) & `1V8_DIGITAL` (`DVDD`), shares 1 reel |
| `R9`, `R10` | `2.2kΩ` 1% | 0603 | **C4190** | **Basic** | 2 | $0.002 | $0.004 | I2C pull-ups (`GPIO0` SDA, `GPIO1` SCL) |
| `R11` | `10kΩ` 1% | 0603 | **C25804** | **Basic** | 1 | $0.002 | $0.002 | Codec `RESET_N` pull-down (reused Basic C-ID) |
| `R12`–`R16` | `33Ω` 1% | 0603 | **C23140** | **Basic** | 5 | $0.002 | $0.010 | I2S signal series damping resistors |
| `C8` | `10µF` 25V X5R | 0805 | **C15850** | **Basic** | 1 | $0.044 | $0.044 | Codec `REF` pin cap |
| `C9`–`C14` | `4.7µF` 16V X5R | 0603 | **C19666** | **Basic** | 6 | $0.017 | $0.102 | Input/output caps for `U11`, `U12`, `U13` |
| `C15`–`C18` | `100nF` 50V X7R | 0603 | **C14663** | **Basic** | 4 | $0.015 | $0.060 | Codec power pin bypass (reused Basic C-ID) |
| **Subtotal Codec & Power** | | | | | **23** | | **~$2.19** | |

---

### B. Isolated Analog RX / TX Front-End

| Ref | Value / Description | Package | LCSC Part # | JLCPCB Type | Qty | Est. Unit Price (USD) | Est. Total (USD) | Notes |
|:---|:---|:---|:---|:---|:---:|:---:|:---:|:---|
| `T_RX`, `T_TX` | `SM-LP-5001E` 600Ω:600Ω Audio Transformer 1:1 | SMD-6 | **C840532** | Extended | 2 | $2.35 | $4.70 | Subminiature SMT telecom 1:1 audio isolation, shares 1 reel |
| `D2`, `D3` | `1N5819WS` (Schottky 40V 1A) | SOD-323 | **C191023** | **Basic** | 2 | $0.012 | $0.024 | Anti-parallel ±0.3V clamp on `RX_SEC` |
| `C_RX1` | `1µF` 50V X5R | 0603 | **C15849** | **Basic** | 1 | $0.015 | $0.015 | DC blocking cap on intercom line (`C4`) |
| `R_RX1` | `47kΩ` 1% 1/10W | 0603 | **C101355** | **Basic** | 1 | $0.007 | $0.007 | RX input attenuator (~ -39.6 dB pad) (`R22`) |
| `R_RX2`–`R_RX4` | `1kΩ` 1% | 0603 | **C115325** | **Basic** | 3 | $0.002 | $0.006 | Termination & `IN1_L` series resistors (`R18, R19, R23`) |
| `C_RX2` | `1µF` 50V X5R | 0603 | **C15849** | **Basic** | 1 | $0.015 | $0.015 | Codec `IN1_L` AC coupling (`C20`) |
| `C_RX3` | `470pF` 50V X7R | 0603 | **C1620** | **Basic** | 1 | $0.006 | $0.006 | Input RF filter capacitor (`C21`) |
| `C_TX1` | `10µF` 25V X5R | 0603 | **C96446** | **Basic** | 1 | $0.037 | $0.037 | AC coupling from `LOL` to `T_TX` (`C22`) |
| `R_TX1` | `100Ω` 1% | 0603 | **C101252** | **Basic** | 1 | $0.002 | $0.002 | `T_TX` primary series drive resistor (`R20`) |
| `C_TX2` | `10µF` 50V X5R | 1206 | **C13585** | **Basic** | 1 | $0.045 | $0.045 | DC blocking cap on intercom TX line (`C5`) |
| `R_TX2` | `4.7kΩ` 1% | 0603 | **C23162** | **Basic** | 1 | $0.003 | $0.003 | Transmit attenuation pad resistor (`R24`) |
| `R_TALK` | `47Ω` 1W 1% | 2512 | **C5154725** | Extended | 1 | $0.16 | $0.16 | Emulates IR204E talk load (`R25`) |
| **Subtotal Front-End** | | | | | **15** | | **~$1.68** | |

---

### C. Talk / Listen Bus Switching (Solid-State / Zero Extra Feeder Fees)

| Ref | Value / Description | Package | LCSC Part # | JLCPCB Type | Qty | Est. Unit Price (USD) | Est. Total (USD) | Notes |
|:---|:---|:---|:---|:---|:---:|:---:|:---:|:---|
| `U14`, `U15` | `AQY212SX` (PhotoMOS Relay) | SOP-4 | **C719754** | Extended | 2 | $0.99 | $1.98 | **Reuses `U7` C-ID ($0 extra feeder fee)** |
| `R17`, `R18` | `680Ω` 1% | 0603 | **C23228** | **Basic** | 2 | $0.002 | $0.004 | **Reuses `R7` value (Basic)** |
| `R19`–`R21` | `100kΩ` 1% | 0603 | **C25803** | **Basic** | 3 | $0.003 | $0.009 | Pull-downs on `GPIO7`, `GPIO10`, `GPIO18` |
| **Subtotal Switching** | | | | | **7** | | **~$1.99** | |

---

## Cost Summary & Assembly Feeder Calculations

| Section | Component Count | Estimated Parts Cost (USD) |
|:---|:---:|:---:|
| **Baseline (Intercom-HEAD Rev 1.1 with U4 unified)** | 23 | ~$6.15 |
| **Audio Codec & Power Tree** | 23 | ~$2.19 |
| **Isolated Analog Front-End** | 15 | ~$1.68 |
| **Talk / Listen Switching (Solid-State)** | 7 | ~$1.99 |
| **Total Components per Board** | **68** | **~$12.01** |

---

### JLCPCB SMT Extended Feeder Loading Fees

JLCPCB charges **$3.00 per unique extended part** reel loaded onto the pick-and-place machine:

1. **Baseline Parts (6 unique extended line items):**
   - `ESP32-C6-WROOM-1-N8` (`C5366877`)
   - `USB_C` (`C709357`)
   - `ME6211C33M5G-N` (`C82942`, **shared across `U4` and `U11` → only 1 feeder fee!**)
   - `AQY212SX` (`C719754`, **shared across `U7`, `U14`, `U15` → only 1 feeder fee!**)
   - `HCPL-817(AT)` (`C48686289`)
   - `MB6S` (`C2886734`)
   - `J1` Screw Terminal (`C474898`)

2. **New Extended Parts for Rev 2.0 Audio (4 unique extended line items):**
   - `TLV320AIC3204IRHBR` Codec (`C24109`) = $3.00
   - `ME6211C18M5G-N` 1.8V LDO (`C236671`, shared between `U12` & `U13`) = $3.00
   - `SM-LP-5001E` SMT Audio Transformer (`C840532`, shared between `T1` & `T2`) = $3.00
   - `R_TALK` 47Ω 1W 2512 Power Resistor (`C5154725`) = $3.00

*(Note: `C5` is now the Basic 10µF 50V part `C13585`, eliminating an extended feeder fee!)*

**Total New Extended Setup Fees for Audio:** **$12.00 per order** (reduced from $21.00!). Distributed across batch: ~$1.20/board for 10 boards.
