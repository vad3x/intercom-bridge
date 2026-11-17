# Spec Delta

## Purpose

Defines the delta requirements for migrating audio streaming from the constrained single-core ESP32-C6 to the dual-core ESP32-S31 with external PSRAM, resolving call-stack exhaustion, CPU starvation during network bursts, jitter buffer memory limits, and direction switching clicks.

## ADDED Requirements

### Requirement: Sustained Voiced Audio Stability Without Call-Stack Exhaustion
The audio pipeline SHALL execute continuous Opus SILK encoding during high-amplitude voiced speech without task stack overflow or heap memory exhaustion.

#### Scenario: Continuous high-pitch voiced audio streaming
- **WHEN** continuous voiced audio requiring recursive SILK pitch analysis (silk_pitch_analysis_core) is streamed through WebRTC for longer than 15 minutes
- **THEN** Opus encoding SHALL complete every 20 millisecond frame without stack overflow, maintaining at least 16 KiB of unused stack headroom at all times

### Requirement: PSRAM Network Jitter Buffering with Packet Loss Concealment
The WebRTC audio receiver SHALL maintain an adaptive sequence-ordered jitter buffer in external PSRAM absorbing up to 100 milliseconds (5 frames) of Wi-Fi latency jitter, emitting concealment frames on packet loss.

#### Scenario: Network delay jitter absorption and packet loss
- **WHEN** inbound SRTP audio packets experience network arrival jitter up to 100 milliseconds or packet loss
- **THEN** the jitter buffer SHALL deliver continuous PCM frames to speaker DMA without underflow, triggering Opus Packet Loss Concealment (PLC) on lost packets

### Requirement: Non-Blocking Unified WebRTC Opus Pipeline
The audio subsystem SHALL serve both Matter Audio Doorbell and the Web Dashboard exclusively via WebRTC Opus streams, maintaining 20 millisecond frame deadlines on Core 1 without raw PCM streaming on Core 0.

#### Scenario: Simultaneous Matter and Web Dashboard audio sessions
- **WHEN** audio is streamed to a WebRTC client during concurrent high-throughput network activity or Matter TCP handshakes
- **THEN** microphone capture, Opus encoding on Core 1, and packet transmission on Core 0 SHALL maintain continuous 20 millisecond cadence with zero dropped frames

### Requirement: Click-Free Direction Switching Soft Ramps
The audio driver SHALL apply fixed-point 10 millisecond linear mute and unmute ramps around analog bus relay transitions to prevent audible pops and clicks during turn-taking.

#### Scenario: Direction transition from listen to talk
- **WHEN** the resident initiates a talk stream while in lobby listen mode
- **THEN** the audio pipeline SHALL apply a 10 millisecond linear fade-out to Terminal 3 capture, switch the hardware relay, wait a 30 millisecond settling delay, and apply a 10 millisecond fade-in to Terminal 2 playback
