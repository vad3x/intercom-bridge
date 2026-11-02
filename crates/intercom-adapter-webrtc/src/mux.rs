//! RFC 7983 first-byte classification for multiplexed WebRTC UDP packets.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DemuxedPacket {
    Stun,
    Dtls,
    RtpOrRtcp,
    Unknown,
}

pub const fn classify_rfc7983_packet(packet: &[u8]) -> DemuxedPacket {
    match packet.first() {
        Some(0..=3) => DemuxedPacket::Stun,
        Some(20..=63) => DemuxedPacket::Dtls,
        Some(128..=191) => DemuxedPacket::RtpOrRtcp,
        _ => DemuxedPacket::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::{classify_rfc7983_packet, DemuxedPacket};

    #[test]
    fn classifies_stun_dtls_and_srtp_first_bytes_per_rfc7983() {
        // Arrange
        let stun_packet: &[u8] = &[0x00, 0x01];
        let dtls_packet: &[u8] = &[22, 0xfe, 0xfd];
        let rtp_packet: &[u8] = &[0x80, 111];
        let empty_packet: &[u8] = &[];

        // Act
        let actual = (
            classify_rfc7983_packet(stun_packet),
            classify_rfc7983_packet(dtls_packet),
            classify_rfc7983_packet(rtp_packet),
            classify_rfc7983_packet(empty_packet),
        );

        // Assert
        assert_eq!(
            actual,
            (
                DemuxedPacket::Stun,
                DemuxedPacket::Dtls,
                DemuxedPacket::RtpOrRtcp,
                DemuxedPacket::Unknown,
            )
        );
    }

    #[test]
    fn classifies_rfc7983_range_boundaries() {
        // Arrange
        let packets = [
            [0],
            [3],
            [4],
            [19],
            [20],
            [63],
            [64],
            [127],
            [128],
            [191],
            [192],
        ];

        // Act
        let actual = packets.map(|packet| classify_rfc7983_packet(&packet));

        // Assert
        assert_eq!(
            actual,
            [
                DemuxedPacket::Stun,
                DemuxedPacket::Stun,
                DemuxedPacket::Unknown,
                DemuxedPacket::Unknown,
                DemuxedPacket::Dtls,
                DemuxedPacket::Dtls,
                DemuxedPacket::Unknown,
                DemuxedPacket::Unknown,
                DemuxedPacket::RtpOrRtcp,
                DemuxedPacket::RtpOrRtcp,
                DemuxedPacket::Unknown,
            ]
        );
    }
}
