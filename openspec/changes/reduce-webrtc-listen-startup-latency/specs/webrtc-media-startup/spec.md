# Spec Delta

## Purpose

Defines fast startup behavior for WebRTC Listen sessions so DTLS identity provisioning, handshake datagram exchange, and initial media onset complete with minimal latency.

## ADDED Requirements

### Requirement: Cached DTLS Identity Provisioning
The system SHALL generate and cache its local ECDSA P-256 DTLS identity (keypair, self-signed X.509 certificate, and SHA-256 fingerprint) once at startup and reuse that cached identity across SDP signaling and DTLS media sessions without regenerating keys or certificates per session.

#### Scenario: ProvideOffer and session startup reuse cached DTLS identity
- **WHEN** a controller sends a WebRTC `ProvideOffer` command to start a Listen session
- **THEN** the SDP answer includes the SHA-256 fingerprint of the cached DTLS certificate without running keypair generation or certificate signing during `ProvideOffer` or media session initialization

#### Scenario: Consecutive Listen sessions preserve matching DTLS fingerprint
- **WHEN** a Listen session ends and a subsequent Listen session starts
- **THEN** both sessions advertise and authenticate with the same valid self-signed ECDSA P-256 DTLS certificate fingerprint

### Requirement: Multi-Datagram DTLS Handshake Flushing
During an active DTLS handshake, the system SHALL drain and transmit all pending outbound DTLS datagrams produced by a handshake step without waiting for a periodic media polling interval between datagrams in the same flight.

#### Scenario: Multi-datagram DTLS server flight is flushed in a single step
- **WHEN** a DTLS handshake step produces multiple outbound datagrams (such as ServerHello, Certificate, ServerKeyExchange, CertificateRequest, and ServerHelloDone)
- **THEN** all queued outbound datagrams for that flight are popped and sent over UDP immediately in the same handshake step

### Requirement: Immediate Bench Media Onset
When running with the embedded bench test recording, the system SHALL emit audible speech frames immediately upon SRTP readiness without inserting a multi-second leading silence pause.

#### Scenario: Bench test recording starts immediately on SRTP ready
- **WHEN** SRTP becomes ready on a bench target using the embedded test speech recording
- **THEN** the first encoded outbound RTP audio frames contain active speech rather than a 1.0 s leading silence gap

### Requirement: Startup Latency Benchmark Reporting
The system SHALL measure and report Listen session startup timings (DTLS initialization duration, DTLS handshake completion duration, and time to first outbound SRTP audio packet) via its media benchmark telemetry and host benchmark tests.

#### Scenario: Benchmark telemetry reports startup latency breakdown
- **WHEN** a Listen session completes DTLS/SRTP setup and transmits its first outbound SRTP audio packet
- **THEN** the media benchmark statistics record and log the DTLS init duration, DTLS handshake duration, and startup latency to first SRTP packet

