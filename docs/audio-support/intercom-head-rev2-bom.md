# Intercom-HEAD Rev 2.0 (Audio Support) — EasyEDA / LCSC Bill of Materials

Cost-optimized BOM for **Intercom-HEAD Rev 2.0** with isolated two-way audio support, mapped to **EasyEDA / JLCPCB / LCSC Part Numbers (C-IDs)**.

---

## Cost Optimization Strategy (JLCPCB SMT Assembly)

1. **Maximize JLCPCB Basic Parts:** Basic parts incur **$0.00 feeder setup fees**. All standard 0603/0805 passives, diodes, switches, and the ME6211 audio LDOs are mapped to Basic Library C-IDs.
2. **Re-use Existing C-IDs:** Reusing components already on `Intercom-HEAD` Rev 1.1 (such as `AQY212SX` for Talk/Listen switching, `680Ω` drive resistors, and `100nF`/`1µF`/`10µF` caps) avoids paying extra Extended Part setup fees ($1.50–$3.00 per unique line item).
3. **Solid-State PhotoMOS over Mechanical Relays:** Reusing the `AQY212SX` PhotoMOS relay eliminates the need for mechanical relay coils, coil driver transistors (`2N3904`), flyback diodes, and the `SN74LVC125A` buffer IC.
4. **Cold Boot & Auto-Flash Reliability Fixes (HW-01 & HW-02):**
   - **Remove `C3` (100nF) on `GPIO9`** and add an external `10kΩ` pull-up (`R6` reused C-ID) to prevent the ESP32-C6 from intermittently entering ROM Download Boot on cold power-up.
   - **Change `C5` on `EN` from `1µF` to `100nF`:** Speeds up the RC reset rise time so the internal USB Serial/JTAG controller can reset the chip cleanly into download mode automatically during `espflash` / `cargo run` without requiring any manual button presses.

---

## 1. Baseline Components (Retained from `Intercom-HEAD` Rev 1.1)

| Ref | Value / Description | Package | LCSC Part # | JLCPCB Type | Qty | Est. Unit Price (USD) | Est. Total (USD) | Notes |
|:---|:---|:---|:---|:---|:---:|:---:|:---:|:---|
| `ESP-32-C6` | `ESP32-C6-WROOM-1-N8` | SMD-29 | **C5283181** | Preferred | 1 | $1.75 | $1.75 | 8MB Flash RISC-V Module |
| `USB_C` | USB Type-C 16-Pin Receptacle | SMD | **C2765186** | **Basic** | 1 | $0.15 | $0.15 | 5V Power + Native USB Serial/JTAG (Auto-flash enabled) |
| `U4` | `AP2112K-3.3TRG1` (600mA LDO) | SOT-23-5 | **C39734** | **Basic** | 1 | $0.14 | $0.14 | 3.3V System LDO |
| `U7` | `AQY212SX` (PhotoMOS Relay) | SOP-4 | **C116744** | Extended | 1 | $0.85 | $0.85 | Door release contact (`GPIO18`) |
| `U8` | `HCPL-817(AT)` / `PC817C` | SOP-4 | **C4998** | **Basic** | 1 | $0.04 | $0.04 | Ring optocoupler detector (`GPIO2`) |
| `D1` | `MB6S` (0.5A 600V Bridge) | SOIC-4 | **C8014** | **Basic** | 1 | $0.05 | $0.05 | Ring AC/DC bridge |
| `LED` | Red LED 0805 | 0805 | **C84256** | **Basic** | 1 | $0.02 | $0.02 | Status indicator (`GPIO19`) |
| `BOOT`, `RESET`| 3×4×2.5mm Tactile Switch | SMD | **C115358** | **Basic** | 2 | $0.04 | $0.08 | Reset & Boot switches |
| `R1`, `R2` | `5.1kΩ` 1% | 0603 | **C23186** | **Basic** | 2 | $0.003 | $0.01 | USB-C CC pulldowns |
| `R3` | `2kΩ` 1% | 0603 | **C4190** | **Basic** | 1 | $0.003 | $0.003 | LED current limiter |
| `R4`, `R5`, `R6` | `10kΩ` 1% | 0603 | **C25804** | **Basic** | 3 | $0.003 | $0.01 | `EN`, `GPIO8`, and **`GPIO9` pull-up for cold boot & auto-flash** (reused C-ID) |
| `R7` | `680Ω` 1% | 0603 | **C23214** | **Basic** | 1 | $0.003 | $0.003 | `U7` PhotoMOS drive resistor |
| `R8` | `6.8kΩ` 1% | 0603 | **C23220** | **Basic** | 1 | $0.003 | $0.003 | Ring optocoupler input resistor |
| `C1`, `C2` | `2.2µF` 16V X5R | 0603 | **C23630** | **Basic** | 2 | $0.008 | $0.02 | `U4` AP2112 filter caps |
| `C4`, `C5`, `C7`| `100nF` 50V X7R | 0603 | **C14663** | **Basic** | 3 | $0.004 | $0.01 | MCU bypass & `RESET` RC delay (**`C3` removed; `C5` changed from 1µF to 100nF for auto-flash**) |
| `C6` | `22µF` 16V X5R | 0805 | **C45783** | **Basic** | 1 | $0.025 | $0.03 | MCU 3.3V bulk filter |
| `J1` | 4-Pin 3.5mm/3.81mm Screw Terminal Block (or 2.54mm) | TH | **C474898** (or **C393325**) | Extended | 1 | $0.25 | $0.25 | Building riser screw terminal (`1, 2, 3, X`) |
| **Subtotal Baseline** | | | | | **23** | | **~$3.43** | |

---

## 2. New Audio Additions (Delta BOM for Rev 2.0)

### A. Codec & Clean Power Tree

| Ref | Value / Description | Package | LCSC Part # | JLCPCB Type | Qty | Est. Unit Price (USD) | Est. Total (USD) | Notes |
|:---|:---|:---|:---|:---|:---:|:---:|:---:|:---|
| `U10` | `TLV320AIC3204IRHBR` | VQFN-32 | **C245642** | Extended | 1 | $3.20 | $3.20 | Low-power audio codec (ADC/DAC) |
| `U11` | `ME6211C33M5G` (3.3V Low-Noise LDO) | SOT-23-5 | **C82942** | **Basic** | 1 | $0.08 | $0.08 | Codec `3V3_CODEC` rail (replaces $1.50 TPS7A) |
| `U12`, `U13` | `ME6211C18M5G` (1.8V Low-Noise LDO) | SOT-23-5 | **C38064** | **Basic** | 2 | $0.08 | $0.16 | Codec `1V8_ANALOG` (`AVDD`) & `1V8_DIGITAL` (`DVDD`) |
| `R9`, `R10` | `2.2kΩ` 1% | 0603 | **C4197** | **Basic** | 2 | $0.003 | $0.01 | I2C pull-ups (`GPIO0` SDA, `GPIO1` SCL) |
| `R11` | `10kΩ` 1% | 0603 | **C25804** | **Basic** | 1 | $0.003 | $0.003 | Codec `RESET_N` pull-down (reused C-ID) |
| `R12`–`R16` | `33Ω` 1% | 0603 | **C25111** | **Basic** | 5 | $0.003 | $0.015 | I2S signal series damping resistors |
| `C8` | `10µF` 16V X5R | 0805 | **C15850** | **Basic** | 1 | $0.015 | $0.015 | Codec `REF` pin cap |
| `C9`–`C14` | `4.7µF` 16V X5R | 0603 | **C19666** | **Basic** | 6 | $0.012 | $0.07 | Input/output caps for `U11`, `U12`, `U13` |
| `C15`–`C18` | `100nF` 50V X7R | 0603 | **C14663** | **Basic** | 4 | $0.004 | $0.02 | Codec power pin bypass (reused C-ID) |
| **Subtotal Codec & Power** | | | | | **23** | | **~$3.57** | |

### B. Isolated Analog RX / TX Front-End

| Ref | Value / Description | Package | LCSC Part # | JLCPCB Type | Qty | Est. Unit Price (USD) | Est. Total (USD) | Notes |
|:---|:---|:---|:---|:---|:---:|:---:|:---:|:---|
| `T_RX`, `T_TX` | 600Ω:600Ω Audio Transformer 1:1 | SMD / TH | **C2900762** | Extended | 2 | $0.65 | $1.30 | Telecom 1:1 audio isolation |
| `D2`, `D3` | `1N5819WS` (Schottky Diode 40V 1A) | SOD-323 | **C2128** | **Basic** | 2 | $0.01 | $0.02 | Anti-parallel ±0.3V clamp on `RX_SEC` (Basic part!) |
| `C_RX1` | `1µF` 100V X7R | 1206 | **C96942** | Extended | 1 | $0.09 | $0.09 | DC blocking cap on 30V intercom line |
| `R_RX1` | `47kΩ` 5% 1/4W | 1206 | **C17937** | **Basic** | 1 | $0.005 | $0.005 | RX input attenuator (~ -39.6 dB pad) |
| `R_RX2`–`R_RX4` | `1kΩ` 1% | 0603 | **C21190** | **Basic** | 3 | $0.003 | $0.01 | Termination & `IN1_L` series resistors |
| `C_RX2` | `1µF` 25V X7R | 0603 | **C15849** | **Basic** | 1 | $0.005 | $0.005 | Codec `IN1_L` AC coupling (reused C-ID) |
| `C_RX3` | `470pF` 50V C0G/NP0 | 0603 | **C1605** | **Basic** | 1 | $0.004 | $0.004 | Input RF filter capacitor |
| `C_TX1` | `10µF` 16V X5R | 0805 | **C15850** | **Basic** | 1 | $0.015 | $0.015 | AC coupling from `LOL` to `T_TX` (reused C-ID) |
| `R_TX1` | `100Ω` 1% | 0603 | **C22775** | **Basic** | 1 | $0.003 | $0.003 | `T_TX` primary series drive resistor |
| `C_TX2` | `22µF` 50V Nonpolar / X7R | 1210 | **C96943** | Extended | 1 | $0.15 | $0.15 | DC blocking cap on intercom TX line |
| `R_TX2` | `4.7kΩ` 1% | 0603 | **C23163** | **Basic** | 1 | $0.003 | $0.003 | Transmit attenuation pad resistor |
| `R_TALK` | `47Ω` 2W 5% | 2512 | **C61633** | **Basic** | 1 | $0.03 | $0.03 | Emulates IR204E talk load (Basic 2512!) |
| **Subtotal Front-End** | | | | | **15** | | **~$1.63** | |

### C. Talk / Listen Bus Switching (Solid-State / Zero Feeder Fees)

| Ref | Value / Description | Package | LCSC Part # | JLCPCB Type | Qty | Est. Unit Price (USD) | Est. Total (USD) | Notes |
|:---|:---|:---|:---|:---|:---:|:---:|:---:|:---|
| `U14`, `U15` | `AQY212SX` (PhotoMOS Relay) | SOP-4 | **C116744** | Extended | 2 | $0.85 | $1.70 | **Reuses `U7` C-ID ($0 extra feeder fee)** |
| `R17`, `R18` | `680Ω` 1% | 0603 | **C23214** | **Basic** | 2 | $0.003 | $0.01 | **Reuses `R7` C-ID** |
| `R19`–`R21` | `100kΩ` 1% | 0603 | **C25803** | **Basic** | 3 | $0.003 | $0.01 | Pull-downs on `GPIO7`, `GPIO10`, `GPIO18` |
| **Subtotal Switching** | | | | | **7** | | **~$1.72** | |

---

## Cost Summary

| Section | Component Count | Estimated Parts Cost (USD) |
|:---|:---:|:---:|
| **Baseline (Intercom-HEAD Rev 1.1)** | 23 | ~$3.30 |
| **Audio Codec & Power Tree** | 23 | ~$3.57 |
| **Isolated Analog Front-End** | 15 | ~$1.63 |
| **Talk / Listen Switching (Solid-State)** | 7 | ~$1.72 |
| **Total Components per Board** | **68** | **~$10.22** |

*(Note: In batches of 5–10 prototype boards, JLCPCB SMT extended feeder loading fee is ~$1.50 per unique extended part: `ESP32-C6` [$1.50] + `AQY212SX` [$1.50] + `TLV320AIC3204` [$3.00] + `Transformer` [$1.50] + `C_TX2` [$1.50] + `C_RX1` [$1.50] + `JST-SH` [$1.50] = ~$12.00 total SMT setup fee distributed across the batch).*
