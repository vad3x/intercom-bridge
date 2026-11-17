# Spec Delta

## Purpose

Defines real-time half-duplex audio intercom communication between the physical analog intercom bus and Matter/WebRTC clients, including Opus codec processing, adaptive jitter buffering, and direction switching matching the IR204E building amplifier.

## ADDED Requirements

### Requirement: Half-Duplex Direction Switching
The audio pipeline SHALL switch between lobby listen mode (terminal 3 capture to WebRTC outbound) and apartment talk mode (WebRTC inbound to terminal 2 playback) without audio clipping or signal leakage between channels.

#### Scenario: Switching from listen to talk
- **WHEN** a WebRTC talk stream is initiated by the controller while in listen mode
- **THEN** the audio pipeline SHALL pause lobby microphone capture, engage the talk relay to terminal 2, and route decoded inbound audio to the building amplifier within 50 milliseconds

### Requirement: Opus Codec Framing and Bitrate
The audio pipeline SHALL encode microphone audio and decode inbound peer audio using the Opus codec in VoIP mode with 20 millisecond frame sizes at 16 kHz or 8 kHz sampling rates.

#### Scenario: Opus encoding time budget
- **WHEN** a 20 millisecond PCM audio frame is captured from the I2S microphone bus
- **THEN** the DSP core SHALL complete Opus encoding within less than 5 milliseconds, leaving sufficient processing margin for networking and system tasks

### Requirement: WebRTC Audio Jitter Buffering
The audio pipeline SHALL maintain an adaptive jitter buffer in external PSRAM to absorb network packet delay variations and packet reordering without causing audio underruns or buffer exhaustion.

#### Scenario: Network delay jitter handling
- **WHEN** incoming SRTP audio packets arrive with network jitter of up to 60 milliseconds
- **THEN** the jitter buffer SHALL deliver continuous, glitch-free PCM audio frames to the I2S speaker DMA without buffer underflow or audible clipping
