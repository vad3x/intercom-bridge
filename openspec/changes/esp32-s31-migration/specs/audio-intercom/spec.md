# Spec Delta

## Purpose

Defines the delta requirements for migrating audio streaming from the constrained single-core ESP32-C6 to the dual-core ESP32-S31 with external PSRAM, resolving call-stack exhaustion, CPU starvation during network bursts, and jitter buffer memory limits.

## ADDED Requirements

### Requirement: Sustained Voiced Audio Stability Without Call-Stack Exhaustion
The audio pipeline SHALL execute continuous Opus SILK encoding during high-amplitude voiced speech without task stack overflow or heap memory exhaustion.

#### Scenario: Continuous high-pitch voiced audio streaming
- **WHEN** continuous voiced audio requiring recursive SILK pitch analysis (silk_pitch_analysis_core) is streamed through WebRTC for longer than 15 minutes
- **THEN** Opus encoding SHALL complete every 20 millisecond frame without stack overflow, maintaining at least 16 KiB of unused stack headroom at all times

### Requirement: Expanded PSRAM Network Jitter Buffering
The WebRTC audio receiver SHALL maintain an adaptive jitter buffer in external PSRAM capable of absorbing residential Wi-Fi packet delay variations without consuming internal SRAM.

#### Scenario: Network delay jitter absorption
- **WHEN** inbound SRTP audio packets experience network arrival jitter of up to 100 milliseconds (5 frames)
- **THEN** the PSRAM jitter buffer SHALL deliver continuous, glitch-free PCM frames to the I2S speaker DMA without buffer underflow or internal SRAM allocation

### Requirement: Non-Blocking Audio Execution During Heavy Network I/O
The audio subsystem SHALL maintain real-time 20 millisecond frame deadlines with zero dropped frames or audio clicks during concurrent high-throughput network activity.

#### Scenario: Simultaneous Matter TCP transfer and active listen stream
- **WHEN** a high-throughput network burst, Matter-over-TCP session negotiation, or HTTP dashboard asset transfer occurs during an active audio stream
- **THEN** microphone capture, Opus encoding, and packet transmission SHALL maintain continuous 20 millisecond cadence with zero missed audio deadlines
