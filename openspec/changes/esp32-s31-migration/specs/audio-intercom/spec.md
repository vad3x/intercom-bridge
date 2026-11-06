# Spec Delta

## Purpose

Defines real-time bidirectional audio intercom communication between the physical analog intercom bus and Matter/WebRTC clients, including full-duplex streaming, Acoustic Echo Cancellation (AEC), Opus codec processing, and jitter buffer performance.

## ADDED Requirements

### Requirement: Full-Duplex Bidirectional Audio Streaming
The audio pipeline SHALL maintain concurrent, simultaneous transmission and reception of audio streams between the intercom analog bus and the connected WebRTC peer without software half-duplex muting or turn-taking interlocks.

#### Scenario: Simultaneous speaking and listening
- **WHEN** audio is actively detected on both the intercom microphone and the incoming WebRTC audio stream
- **THEN** both inbound audio from the bus is transmitted via WebRTC and outbound audio from WebRTC is played onto the intercom speaker concurrently without stream interruption

### Requirement: Acoustic Echo Cancellation
The audio pipeline SHALL apply real-time Acoustic Echo Cancellation (AEC) to eliminate acoustic feedback and speaker echo from the intercom microphone input before Opus encoding.

#### Scenario: Echo suppression during loud playback
- **WHEN** incoming WebRTC audio is played through the intercom speaker at peak nominal volume
- **THEN** the echo cancellation filter SHALL attenuate the loudspeaker echo on the microphone capture channel by at least 25 dB before packet transmission

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
