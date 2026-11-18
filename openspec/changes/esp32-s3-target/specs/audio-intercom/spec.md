# Spec Delta

## Purpose

Defines analog intercom capture and playback behavior, audio transport, and the timing and resource requirements of live calls.

## ADDED Requirements

### Requirement: ESP32-S3 Physical Half-Duplex Audio
The supported S3 intercom board SHALL capture and play back 16 kHz mono audio through the external codec and I2S DMA. Listening and talking SHALL use the existing half-duplex relay interlock and direction-switch ramps.

#### Scenario: Listen, talk, and return to idle
- **WHEN** an authenticated WebRTC client starts a supported audio session and changes direction on an S3 board with the codec and isolated relay fixture attached
- **THEN** the remote client SHALL receive captured intercom audio in listen mode and the intercom SHALL play received audio in talk mode
- **AND** the listen and talk relays SHALL remain mutually exclusive and the existing mute ramp and settling sequence SHALL occur around direction changes
- **AND** no media transition SHALL pulse the door relay, and ending the session SHALL return the bus to idle

### Requirement: ESP32-S3 Sustained Audio and Resource Budget
The S3 bridge SHALL sustain 20 millisecond Opus frame deadlines during a 15 minute voiced-audio WebRTC call with concurrent Wi-Fi traffic. The call SHALL maintain at least 32 KiB of free internal heap and 16 KiB of unused audio-worker stack throughout the measured run.

#### Scenario: Measure a sustained Matter WebRTC call
- **WHEN** a Matter controller starts WebRTC listening, including a period longer than 180 seconds, followed by supported talk and listen transitions during concurrent network use
- **THEN** ICE nomination, DTLS fingerprint verification, and authenticated SRTP SHALL complete and physical audio SHALL continue without DMA overruns, worker restarts, or missed audio frame deadlines over the sustained run
- **AND** internal heap and audio-worker stack SHALL stay at or above their minimum free-space budgets
