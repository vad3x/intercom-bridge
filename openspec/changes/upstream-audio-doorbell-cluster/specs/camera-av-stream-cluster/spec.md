# Spec Delta: Camera AV Stream Management Cluster (0x0551) & Audio Doorbell (0x0141)

## Purpose

Enables standard Matter 1.4 audio streaming for doorbell and intercom devices by defining the Camera AV Stream Management cluster (0x0551) and Audio Doorbell device type (0x0141) within rs-matter data model.

## ADDED Requirements

### Requirement: Standard Audio Doorbell Device Type Definition
The data model SHALL define the Matter 1.4 Audio Doorbell device type with identifier 0x0141 and device revision 1.

#### Scenario: Audio doorbell device type registration
- **WHEN** an endpoint is configured as an Audio Doorbell
- **THEN** its descriptor cluster reports device type identifier 0x0141 with revision 1

### Requirement: Audio AV Stream Cluster Handler Scaffolding
The system SHALL provide an audio-only cluster handler for cluster ID 0x0551 advertising features Audio (bit 1) and Speaker (bit 2) while omitting Video and Snapshot feature bits.

#### Scenario: Audio stream cluster feature negotiation
- **WHEN** a Matter controller queries the FeatureMap attribute (0xFFFC) of cluster 0x0551 on an audio doorbell endpoint
- **THEN** the cluster reports the bitwise OR of Feature::AUDIO and Feature::SPEAKER
- **AND** Feature::VIDEO and Feature::SNAPSHOT bits evaluate to 0

### Requirement: Mandatory Global Cluster Attributes
The cluster handler SHALL implement all standard Matter global attributes (0xFFF8 through 0xFFFD) including FeatureMap, ClusterRevision, AttributeList, AcceptedCommandList, and GeneratedCommandList.

#### Scenario: Controller reads global attribute list
- **WHEN** a Matter controller queries the AttributeList attribute (0xFFFB) of cluster 0x0551
- **THEN** the returned list contains all implemented cluster-specific attributes and global attributes 0xFFF8 through 0xFFFD

### Requirement: Allocate Audio Stream Command Handling
The cluster handler SHALL accept the AllocateAudioStream command for supported audio codecs and stream usages, assigning a non-zero stream identifier.

#### Scenario: Successful audio stream allocation
- **WHEN** a client invokes AllocateAudioStream requesting live view usage and mono Opus codec at 16 kHz or 48 kHz
- **THEN** the handler allocates a stream record, assigns a unique non-zero stream ID, and returns an AllocateAudioStreamResponse with that ID
- **AND** the handler invokes the registered allocation hook callback

#### Scenario: Unsupported codec rejected
- **WHEN** a client invokes AllocateAudioStream requesting an unsupported codec such as AAC-LC
- **THEN** the handler rejects the command with a DynamicConstraintError status code

### Requirement: Stream ID Wrapping and Monotonic Assignment
The stream allocation engine SHALL monotonically increment stream IDs and wrap around to 1, skipping any currently active stream IDs without colliding or exhausting unallocated slots.

#### Scenario: Stream ID counter wrap around
- **WHEN** the stream ID counter reaches 0xFFFF and a new stream allocation request arrives
- **THEN** the handler rolls the counter over to 1
- **AND** if stream ID 1 is currently active, the handler advances to the next available unused ID

### Requirement: Deallocate Audio Stream Command Handling
The cluster handler SHALL accept the DeallocateAudioStream command, validating that the stream exists and has no active transport references before freeing the slot.

#### Scenario: Successful audio stream deallocation
- **WHEN** a client invokes DeallocateAudioStream for an existing stream with zero active transport references
- **THEN** the handler removes the stream record and invokes the deallocation hook callback

#### Scenario: Deallocation blocked while transport session active
- **WHEN** a client invokes DeallocateAudioStream for a stream currently referenced by an active WebRTC session
- **THEN** the handler rejects the request with an InvalidInState status code

### Requirement: Transport Reference Counting Safety
The audio stream lifecycle SHALL track active transport leases via an explicit reference count, saturating safely and returning an error on overflow.

#### Scenario: Reference count saturation protection
- **WHEN** an acquire request is received for a stream whose reference count is already at 255
- **THEN** the handler rejects the acquire with a ResourceExhausted error
- **AND** the reference count remains 255 without integer overflow
