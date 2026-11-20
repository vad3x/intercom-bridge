//! Fixed-capacity, sequence-ordered Opus playout buffer.

use intercom_media::OpusPacket;

#[derive(Clone, Copy)]
struct Entry {
    sequence: u16,
    packet: OpusPacket,
}

/// Fixed-window jitter buffer. Call [`Self::pop`] once per playout interval;
/// an empty slot advances the sequence and returns `None` for Opus PLC.
pub struct JitterBuffer<const N: usize> {
    slots: [Option<Entry>; N],
    next_sequence: u16,
}

impl<const N: usize> JitterBuffer<N> {
    /// Start playout at the expected first RTP sequence number.
    pub const fn new(first_sequence: u16) -> Self {
        assert!(N > 0, "jitter buffer capacity must be nonzero");
        Self {
            slots: [None; N],
            next_sequence: first_sequence,
        }
    }

    /// Discard queued packets and begin playout at a newly negotiated sequence.
    pub fn reset(&mut self, first_sequence: u16) {
        self.slots = [None; N];
        self.next_sequence = first_sequence;
    }

    /// Insert a packet within the current playout window.
    ///
    /// Returns `false` for stale, duplicate, or too-far-ahead sequence numbers.
    pub fn insert(&mut self, sequence: u16, packet: OpusPacket) -> bool {
        let distance = sequence.wrapping_sub(self.next_sequence);
        if distance >= 0x8000 || usize::from(distance) >= N {
            return false;
        }

        let slot = &mut self.slots[usize::from(distance)];
        if slot.is_some() {
            return false;
        }
        *slot = Some(Entry { sequence, packet });
        true
    }

    /// Emit the next packet in sequence, or `None` on a playout loss for PLC.
    pub fn pop(&mut self) -> Option<OpusPacket> {
        let packet = match self.slots[0].take() {
            Some(entry) if entry.sequence == self.next_sequence => Some(entry.packet),
            _ => None,
        };
        self.slots.copy_within(1..N, 0);
        self.slots[N - 1] = None;
        self.next_sequence = self.next_sequence.wrapping_add(1);
        packet
    }

    /// Return the sequence number that the next playout interval will request.
    pub const fn next_sequence(&self) -> u16 {
        self.next_sequence
    }

    /// Return the number of buffered packets currently awaiting playout.
    pub fn buffered_count(&self) -> usize {
        self.slots.iter().filter(|slot| slot.is_some()).count()
    }

    /// Return `true` when no packets are buffered.
    pub fn is_empty(&self) -> bool {
        self.slots.iter().all(Option::is_none)
    }
}

#[cfg(test)]
mod tests {
    use super::JitterBuffer;
    use intercom_media::{OpusPacket, MAX_OPUS_PACKET_BYTES};

    fn packet(value: u8) -> OpusPacket {
        let mut payload = [0; MAX_OPUS_PACKET_BYTES];
        payload[0] = value;
        OpusPacket { len: 1, payload }
    }

    #[test]
    fn jitter_buffer_orders_reordered_packets_and_emits_loss_for_plc() {
        let mut buffer = JitterBuffer::<5>::new(10);

        assert!(buffer.insert(11, packet(11)));
        assert!(buffer.insert(10, packet(10)));
        assert!(buffer.insert(12, packet(12)));

        assert_eq!(buffer.pop(), Some(packet(10)));
        assert_eq!(buffer.pop(), Some(packet(11)));
        assert_eq!(buffer.pop(), Some(packet(12)));

        assert!(buffer.insert(14, packet(14)));
        assert_eq!(buffer.pop(), None);
        assert_eq!(buffer.pop(), Some(packet(14)));
    }

    #[test]
    fn jitter_buffer_rejects_packets_outside_playout_window() {
        let mut buffer = JitterBuffer::<5>::new(20);

        assert!(!buffer.insert(25, packet(25)));
        assert!(!buffer.insert(19, packet(19)));
        assert!(buffer.insert(24, packet(24)));
    }

    #[test]
    fn jitter_buffer_absorbs_a_100ms_five_frame_arrival_burst() {
        let mut buffer = JitterBuffer::<5>::new(100);

        for sequence in 100..105 {
            assert!(buffer.insert(sequence, packet(sequence as u8)));
        }
        for sequence in 100..105 {
            assert_eq!(buffer.pop(), Some(packet(sequence as u8)));
        }
        for sequence in (105..110).rev() {
            assert!(buffer.insert(sequence, packet(sequence as u8)));
        }
        for sequence in 105..110 {
            assert_eq!(buffer.pop(), Some(packet(sequence as u8)));
        }
    }

    #[test]
    fn jitter_buffer_preserves_packets_across_rtp_sequence_wraparound() {
        let mut buffer = JitterBuffer::<5>::new(u16::MAX);

        assert!(buffer.insert(u16::MAX, packet(1)));
        assert!(buffer.insert(0, packet(2)));

        assert_eq!(buffer.pop(), Some(packet(1)));
        assert_eq!(buffer.pop(), Some(packet(2)));
        assert_eq!(buffer.next_sequence(), 1);
    }
}
