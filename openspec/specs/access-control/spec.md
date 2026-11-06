# Access Control & Intercom Arbitration

## Purpose
Arbitrates door strike unlock, half-duplex audio sessions (Listen and Talk), and visitor ring announcements across smart home ecosystems, local interfaces, and physical intercom line hardware.

## Requirements

### Requirement: Half-Duplex Audio Listen
An authorized client MUST be able to open an audio session to listen to the outdoor intercom station.

#### Scenario: Start listen session
- **WHEN** an authorized client starts a listen session
- **THEN** the intercom line transitions to listen mode
- **AND** incoming audio capture is unmuted and streamed to the client
- **AND** the session automatically returns to idle if client heartbeats cease for more than 5 seconds or the maximum listen duration (180 seconds) is exceeded

### Requirement: Push-to-Talk Transmission
An authorized client with an active session MUST be able to switch the intercom line to transmit speech to the outdoor station and automatically revert to listening when finished.

#### Scenario: Push-to-talk toggle
- **WHEN** the client asserts talk mode during an active call
- **THEN** the line transitions through an electrical settling period and enters talk mode
- **AND** outgoing audio transmission is enabled while incoming capture is muted
- **AND** when the client stops talking, the line settles and automatically returns to listen mode

#### Scenario: Talk duration safety limit
- **WHEN** talk mode is sustained continuously beyond the maximum talk duration (60 seconds)
- **THEN** the system automatically ends transmission, settles the line, and returns to listen mode

### Requirement: Intercom Line Mutual Exclusion & Settling
The intercom bus MUST strictly enforce mutual exclusion between audio listen, audio talk, and door strike actuation, inserting an electrical settling delay on every state change.

#### Scenario: Unlock requested during active audio call
- **WHEN** an unlock request is received while an audio session is active
- **THEN** audio transmission/capture is paused immediately
- **AND** the door strike energizes for the configured pulse duration
- **AND** once the strike de-energizes, the line enters an electrical settling period and resumes the active call in listen mode

### Requirement: Door Strike Timed Unlock
When an unlock action is triggered by an authorized client or trigger, the system MUST energize the door strike for a configurable duration and automatically return to the locked state.

#### Scenario: Standalone unlock trigger
- **WHEN** an unlock request is received from an authorized source (Matter, Web Dashboard, or physical action)
- **THEN** the door strike energizes
- **AND** the access state is reported as unlocked to connected smart home controllers and interfaces
- **AND** after the configured unlock duration expires (default 5 seconds, clamped between 1 and 30 seconds), the strike de-energizes and the access state reports locked

### Requirement: Visitor Ring Announcement Hold
When an incoming ring signal is detected on the intercom line, the system MUST announce the visitor chime and hold the event active for a defined window.

#### Scenario: Ring detection and automatic clearing
- **WHEN** an incoming ring voltage is detected on the intercom line
- **THEN** a visitor ring notification is broadcast to all registered controllers and switches
- **AND** the ring state remains asserted for 15 seconds after the last pulse before returning to idle
