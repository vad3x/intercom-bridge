# Hardware Documentation

Complete hardware setup guide for the Smart Intercom project.

## Bill of Materials

### Core Components

| Component | Part Number | Supplier | Qty | Notes |
|-----------|-------------|----------|-----|-------|
| ESP32-C6 Board | DFR1117 | DigiKey | 1 | DFRobot Beetle ESP32-C6 |
| Optocoupler | HCPL-817-000E | DigiKey | 1 | For ring detection |
| PhotoMOS Relay | AQY212EHA | DigiKey | 1 | For door unlock |
| Bridge Rectifier | DB107 | DigiKey | 1 | For AC ring signal |

### Passive Components

| Component | Value | Package | Qty | Notes |
|-----------|-------|---------|-----|-------|
| Resistor | 330Ω | 0805/TH | 1 | Optocoupler LED |
| Resistor | 1kΩ | 0805/TH | 1 | PhotoMOS LED |
| Resistor | 10kΩ | 0805/TH | 1 | GPIO pull-up (optional) |
| Capacitor | 100nF | 0805/TH | 2 | Decoupling |

### Connectors & Wiring

| Component | Type | Qty | Notes |
|-----------|------|-----|-------|
| WAGO 221-412 | 2-conductor lever nut | 4 | Intercom wire tap |
| WAGO 221-413 | 3-conductor lever nut | 2 | Optional |
| Dupont wires | F-F, 10cm | 10 | Board connections |
| Perfboard | 5x7cm | 1 | Circuit mounting |
| Screw terminal | 2-pin, 5.08mm | 2 | Intercom connections |

## Circuit Diagrams

### Ring Detection Circuit

The optocoupler isolates the ESP32 from the intercom's ring voltage (unknown polarity/voltage).

```
                    INTERCOM                          ESP32-C6
                 Ring Signal                             
                                                        
     ┌─────────────┐                              ┌───────────┐
     │             │                              │           │
     │   Ring+  ───┼────┬────────────────────────┼─── 3.3V   │
     │             │    │                         │           │
     │             │    │    ┌────────────┐       │           │
     │             │    │    │  HCPL-817  │       │           │
     │             │    │    │            │       │           │
     │             │    └────┤1 Anode     │       │           │
     │             │         │      Vcc 4├───────┼─── 3.3V   │
     │             │    ┌────┤2 Cathode   │       │           │
     │             │    │    │            │       │           │
     │             │    │    │   Output 3├───────┼─── GPIO2  │
     │   Ring-  ───┼────┤    │            │       │  (BUZZ)   │
     │             │    │    │      GND 4├───────┼─── GND    │
     └─────────────┘    │    └────────────┘       │           │
                        │                         └───────────┘
                        │
                   ┌────┴────┐
                   │   R1    │
                   │  330Ω   │
                   └────┬────┘
                        │
                        ▼
                   ┌────────┐
                   │  DB107 │  Bridge Rectifier
                   │        │  (for AC signals)
                   └────────┘
                        │
                        ▼
                      GND
```

**How it works:**
1. Ring signal (AC or DC) flows through bridge rectifier
2. Rectified current flows through 330Ω resistor into optocoupler LED
3. Optocoupler transistor conducts, pulling GPIO2 LOW
4. ESP32 detects falling edge interrupt
5. Firmware debounces and registers buzz event

**Component selection:**
- 330Ω chosen for ~10mA LED current at 12V input
- Adjust resistor for different intercom voltages:
  - 5V signal: 150Ω
  - 12V signal: 330Ω
  - 24V signal: 680Ω

### Door Unlock Circuit

The PhotoMOS relay acts as an isolated electronic switch in parallel with the door button.

```
                    INTERCOM                          ESP32-C6
                 Door Button                             
                                                        
     ┌─────────────┐                              ┌───────────┐
     │             │                              │           │
     │   Door   ───┼────┬─────────────────────────┼─── 3.3V   │
     │   Button    │    │                         │           │
     │     +       │    │    ┌────────────┐       │           │
     │             │    │    │  AQY212EHA │       │           │
     │    ═══      │    │    │  PhotoMOS  │       │           │
     │   (btn)     │    │    │            │       │           │
     │             │    └────┤4 Load      │       │           │
     │             │         │            │       │           │
     │   Door   ───┼────┬────┤3 Load      │       │           │
     │   Button    │    │    │            │       │           │
     │     -       │    │    │   Anode  1├──┬────┼─── GPIO3  │
     │             │    │    │            │  │    │ (UNLOCK)  │
     └─────────────┘    │    │ Cathode  2├──┼────┼─── GND    │
                        │    └────────────┘  │    │           │
                        │                    │    └───────────┘
                        │                ┌───┴───┐
                        │                │  R2   │
                        │                │  1kΩ  │
                        │                └───────┘
                        │
                     (to intercom
                      door circuit)
```

**How it works:**
1. GPIO3 goes HIGH to trigger unlock
2. Current flows through 1kΩ resistor into PhotoMOS LED
3. PhotoMOS internal MOSFETs conduct (bidirectional)
4. This shorts the door button contacts
5. Intercom sees "button pressed"
6. Door lock releases

**Timing:**
- Pulse duration: 700ms (matches typical button press)
- After pulse, GPIO returns LOW, relay opens

### Complete Wiring Overview

```
┌──────────────────────────────────────────────────────────────┐
│                       INTERCOM UNIT                          │
│                                                              │
│   Ring+ ●────────────────────────────────────────────────────┼──┐
│   Ring- ●────────────────────────────────────────────────────┼──┼──┐
│                                                              │  │  │
│   Door+ ●────────────────────────────────────────────────────┼──┼──┼──┐
│   Door- ●────────────────────────────────────────────────────┼──┼──┼──┼──┐
│                                                              │  │  │  │  │
└──────────────────────────────────────────────────────────────┘  │  │  │  │
            ▲                                                     │  │  │  │
            │ Use WAGO 221 connectors                             │  │  │  │
            │ to tap wires non-destructively                      │  │  │  │
                                                                  │  │  │  │
┌─────────────────────────────────────────────────────────────────┼──┼──┼──┼──┐
│                      PERFBOARD CIRCUIT                          │  │  │  │  │
│                                                                 │  │  │  │  │
│   ┌─────────────────┐                                           │  │  │  │  │
│   │  Bridge Rect.   │◄──────────────────────────────────────────┘  │  │  │  │
│   │     DB107       │◄─────────────────────────────────────────────┘  │  │  │
│   └────────┬────────┘                                                 │  │  │
│            │                                                          │  │  │
│            ▼                                                          │  │  │
│   ┌─────────────────┐                                                 │  │  │
│   │   Optocoupler   │                                                 │  │  │
│   │    HCPL-817     ├─────────────────────────────────────────────┐   │  │  │
│   └─────────────────┘                                             │   │  │  │
│                                                                   │   │  │  │
│   ┌─────────────────┐                                             │   │  │  │
│   │  PhotoMOS Relay │◄────────────────────────────────────────────┼───┘  │  │
│   │    AQY212EHA    │◄────────────────────────────────────────────┼──────┘  │
│   └────────┬────────┘                                             │         │
│            │                                                      │         │
│            └──────────────────────────────────────────────────────┼─────────┘
│                                                                   │
│   DUPONT HEADERS TO ESP32:                                        │
│     • 3.3V ─────────────────────────────────────────────────────► │
│     • GND ──────────────────────────────────────────────────────► │
│     • GPIO2 (Buzz) ─────────────────────────────────────────────► │
│     • GPIO3 (Unlock) ───────────────────────────────────────────► │
│                                                                   │
└───────────────────────────────────────────────────────────────────┘
                                    │
                                    │ Dupont wires
                                    ▼
                    ┌───────────────────────────────┐
                    │    DFRobot Beetle ESP32-C6    │
                    │                               │
                    │   USB-C ──► Power (5V)        │
                    │                               │
                    │   GPIO2 ◄── Buzz Input        │
                    │   GPIO3 ──► Unlock Output     │
                    │   3.3V  ──► Circuit Power     │
                    │   GND   ──► Common Ground     │
                    │                               │
                    └───────────────────────────────┘
```

## Measuring Intercom Voltage

Before finalizing resistor values, measure your intercom's ring signal:

### Safety First
- Use a multimeter rated for the expected voltage
- Never touch exposed wires while powered
- If in doubt, assume higher voltage

### Measurement Procedure

1. Set multimeter to AC Voltage (VAC), 200V range
2. Connect probes to Ring+ and Ring- wires
3. Have someone ring the intercom
4. Note the peak voltage reading
5. If reading is 0, try DC Voltage (VDC)
6. Record both AC and DC readings

### Resistor Calculation

For optocoupler LED (forward voltage ~1.2V, target current 10mA):

```
R = (V_ring - V_led) / I_led
R = (V_ring - 1.2V) / 0.010A
```

| Ring Voltage | Resistor Value |
|--------------|----------------|
| 5V | 380Ω → use 390Ω |
| 9V | 780Ω → use 820Ω |
| 12V | 1080Ω → use 1kΩ |
| 18V | 1680Ω → use 1.8kΩ |
| 24V | 2280Ω → use 2.2kΩ |

## ESP32-C6 Beetle Pinout

DFRobot Beetle ESP32-C6 (DFR1117):

```
                    USB-C
                   ┌─────┐
              3.3V │●   ●│ 5V
               GND │●   ●│ GND  
             GPIO0 │●   ●│ GPIO1
        ──► GPIO2 │●   ●│ GPIO3 ◄──
             GPIO4 │●   ●│ GPIO5
             GPIO6 │●   ●│ GPIO7
                   └─────┘

    GPIO2 = Buzz input (configure as INPUT_PULLUP)
    GPIO3 = Unlock output (configure as OUTPUT)
```

## Testing Procedure

### 1. Bench Test (No Intercom)

1. Power ESP32 via USB
2. Check serial output: `idf.py monitor`
3. Verify HTTP server starts
4. Test API: `curl http://<ip>/status`

### 2. Optocoupler Test

1. Disconnect from intercom
2. Apply 5V to optocoupler input through 330Ω resistor
3. Verify GPIO2 goes LOW
4. Check serial log for "BUZZ DETECTED"

### 3. PhotoMOS Test

1. Disconnect from intercom
2. Measure resistance across PhotoMOS output pins
3. Call `POST /unlock`
4. Resistance should drop to near 0Ω for 700ms

### 4. Integration Test

1. Connect to intercom using WAGO connectors
2. Ring the intercom → verify buzz detection
3. Call unlock API → verify door releases
4. Test from outside the building

## Troubleshooting

### No Buzz Detection

1. Check optocoupler orientation (pin 1 is marked)
2. Measure voltage across optocoupler input
3. Try lower resistor value (increase LED current)
4. Verify GPIO2 pull-up is enabled

### Unlock Not Working

1. Measure PhotoMOS output with multimeter
2. Verify GPIO3 goes HIGH during unlock
3. Check PhotoMOS LED current (should be ~3mA)
4. Try longer pulse duration

### Intermittent Issues

1. Add 100nF capacitor across optocoupler LED
2. Add 100nF decoupling capacitor near ESP32
3. Use shielded cable for long wire runs
4. Check for loose WAGO connections

## Safety Warnings

⚠️ **Electrical Safety**
- Intercom voltages vary (5V to 48V DC or AC)
- Always use isolation (optocoupler + PhotoMOS)
- Never connect intercom directly to ESP32

⚠️ **Building Codes**
- This modification may affect building security
- Consult building management before installation
- Maintain ability to manually unlock

⚠️ **Reversibility**
- WAGO connectors allow complete removal
- Keep original door button functional
- Document your wiring for future reference







