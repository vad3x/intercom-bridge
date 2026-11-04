//! Allocation-free SRTP AES-128-CM/HMAC-SHA1-80 packet protection.

use crate::dtls::{DtlsRole, DtlsSrtpKeys};
use aes::Aes128;
use ctr::cipher::{KeyIvInit, StreamCipher};
use hmac::{Hmac, Mac};
use sha1::Sha1;

const RTP_FIXED_HEADER_BYTES: usize = 12;
const AUTH_TAG_BYTES: usize = 10;
const AUTH_KEY_BYTES: usize = 20;
const SRTCP_INDEX_BYTES: usize = 4;
const SRTCP_FIXED_RR_BYTES: usize = 8;
type HmacSha1 = Hmac<Sha1>;
type Aes128Ctr = ctr::Ctr128BE<Aes128>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SrtpError {
    InvalidRtpPacket,
    InvalidRtcpPacket,
    BufferTooSmall,
    AuthenticationFailed,
    Replay,
    OutOfOrder,
}

pub struct SrtpSession {
    send: SrtpDirection,
    receive: SrtpDirection,
    send_rollover_counter: u32,
    last_sent_sequence: Option<u16>,
    highest_received_index: Option<u64>,
    replay_window: u64,
    send_rtcp_index: u32,
    highest_received_rtcp_index: Option<u32>,
    rtcp_replay_window: u64,
}

#[derive(Clone, Copy)]
struct SrtpDirection {
    encryption_key: [u8; 16],
    authentication_key: [u8; AUTH_KEY_BYTES],
    session_salt: [u8; 14],
    rtcp_encryption_key: [u8; 16],
    rtcp_authentication_key: [u8; AUTH_KEY_BYTES],
    rtcp_session_salt: [u8; 14],
}

impl SrtpSession {
    pub fn new(master_key: [u8; 16], master_salt: [u8; 14]) -> Self {
        let direction = SrtpDirection::from_master(&master_key, &master_salt);
        Self {
            send: direction,
            receive: direction,
            send_rollover_counter: 0,
            last_sent_sequence: None,
            highest_received_index: None,
            replay_window: 0,
            send_rtcp_index: 0,
            highest_received_rtcp_index: None,
            rtcp_replay_window: 0,
        }
    }

    pub fn from_dtls_keys(keys: DtlsSrtpKeys, role: DtlsRole) -> Self {
        let (send_key, send_salt, receive_key, receive_salt) = match role {
            DtlsRole::Client => (
                keys.client_master_key,
                keys.client_master_salt,
                keys.server_master_key,
                keys.server_master_salt,
            ),
            DtlsRole::Server => (
                keys.server_master_key,
                keys.server_master_salt,
                keys.client_master_key,
                keys.client_master_salt,
            ),
        };
        Self {
            send: SrtpDirection::from_master(&send_key, &send_salt),
            receive: SrtpDirection::from_master(&receive_key, &receive_salt),
            send_rollover_counter: 0,
            last_sent_sequence: None,
            highest_received_index: None,
            replay_window: 0,
            send_rtcp_index: 0,
            highest_received_rtcp_index: None,
            rtcp_replay_window: 0,
        }
    }

    pub fn derive_session_keys(
        master_key: &[u8; 16],
        master_salt: &[u8; 14],
    ) -> ([u8; 16], [u8; AUTH_KEY_BYTES], [u8; 14]) {
        let encryption_key = derive_key::<16>(master_key, master_salt, 0x00);
        let authentication_key = derive_key::<AUTH_KEY_BYTES>(master_key, master_salt, 0x01);
        let session_salt = derive_key::<14>(master_key, master_salt, 0x02);
        (encryption_key, authentication_key, session_salt)
    }

    pub fn protect_rtp(
        &mut self,
        packet: &mut [u8],
        packet_length: usize,
    ) -> Result<usize, SrtpError> {
        if packet_length < RTP_FIXED_HEADER_BYTES || packet_length > packet.len() {
            return Err(SrtpError::InvalidRtpPacket);
        }
        if packet[0] >> 6 != 2 {
            return Err(SrtpError::InvalidRtpPacket);
        }
        let payload_start =
            rtp_payload_offset(packet, packet_length).ok_or(SrtpError::InvalidRtpPacket)?;
        let protected_length = packet_length
            .checked_add(AUTH_TAG_BYTES)
            .filter(|length| *length <= packet.len())
            .ok_or(SrtpError::BufferTooSmall)?;
        let sequence = u16::from_be_bytes([packet[2], packet[3]]);
        let rollover_counter = self.next_send_rollover_counter(sequence)?;
        let packet_index = (u64::from(rollover_counter) << 16) | u64::from(sequence);
        let ssrc = u32::from_be_bytes(packet[8..12].try_into().expect("RTP SSRC"));
        self.crypt_payload(
            &self.send,
            packet,
            payload_start,
            packet_length,
            ssrc,
            packet_index,
        );

        let mut mac = HmacSha1::new_from_slice(&self.send.authentication_key)
            .expect("fixed SRTP authentication key length");
        mac.update(&packet[..packet_length]);
        mac.update(&rollover_counter.to_be_bytes());
        packet[packet_length..protected_length]
            .copy_from_slice(&mac.finalize().into_bytes()[..AUTH_TAG_BYTES]);

        self.send_rollover_counter = rollover_counter;
        self.last_sent_sequence = Some(sequence);
        Ok(protected_length)
    }

    pub fn unprotect_rtp(
        &mut self,
        packet: &mut [u8],
        packet_length: usize,
    ) -> Result<usize, SrtpError> {
        if packet_length < RTP_FIXED_HEADER_BYTES + AUTH_TAG_BYTES || packet_length > packet.len() {
            return Err(SrtpError::InvalidRtpPacket);
        }
        if packet[0] >> 6 != 2 {
            return Err(SrtpError::InvalidRtpPacket);
        }
        let payload_end = packet_length - AUTH_TAG_BYTES;
        let payload_start =
            rtp_payload_offset(packet, payload_end).ok_or(SrtpError::InvalidRtpPacket)?;
        let sequence = u16::from_be_bytes([packet[2], packet[3]]);
        let packet_index = self.guess_receive_index(sequence)?;
        self.check_replay_window(packet_index)?;
        let rollover_counter = (packet_index >> 16) as u32;
        let mut mac = HmacSha1::new_from_slice(&self.receive.authentication_key)
            .expect("fixed SRTP authentication key length");
        mac.update(&packet[..payload_end]);
        mac.update(&rollover_counter.to_be_bytes());
        let expected_tag = mac.finalize().into_bytes();
        let mut tag_difference = 0u8;
        for (expected, actual) in expected_tag[..AUTH_TAG_BYTES]
            .iter()
            .zip(&packet[payload_end..packet_length])
        {
            tag_difference |= *expected ^ *actual;
        }
        if tag_difference != 0 {
            return Err(SrtpError::AuthenticationFailed);
        }

        let ssrc = u32::from_be_bytes(packet[8..12].try_into().expect("RTP SSRC"));
        self.crypt_payload(
            &self.receive,
            packet,
            payload_start,
            payload_end,
            ssrc,
            packet_index,
        );
        self.update_replay_window(packet_index);
        Ok(payload_end)
    }

    pub fn protect_rtcp(
        &mut self,
        packet: &mut [u8],
        packet_length: usize,
    ) -> Result<usize, SrtpError> {
        validate_receiver_report(packet, packet_length)?;
        let rtcp_end = packet_length;
        let index_offset = rtcp_end;
        let tag_offset = index_offset + SRTCP_INDEX_BYTES;
        let protected_length = tag_offset
            .checked_add(AUTH_TAG_BYTES)
            .filter(|length| *length <= packet.len())
            .ok_or(SrtpError::BufferTooSmall)?;
        if self.send_rtcp_index > 0x7fff_ffff {
            return Err(SrtpError::OutOfOrder);
        }

        let index = self.send_rtcp_index | 0x8000_0000;
        let ssrc = u32::from_be_bytes(packet[4..8].try_into().expect("RTCP SSRC"));
        self.crypt_rtcp_payload(
            &self.send,
            packet,
            SRTCP_FIXED_RR_BYTES,
            rtcp_end,
            ssrc,
            self.send_rtcp_index,
        );
        packet[index_offset..tag_offset].copy_from_slice(&index.to_be_bytes());
        let mut mac = HmacSha1::new_from_slice(&self.send.rtcp_authentication_key)
            .expect("fixed SRTCP authentication key length");
        mac.update(&packet[..tag_offset]);
        packet[tag_offset..protected_length]
            .copy_from_slice(&mac.finalize().into_bytes()[..AUTH_TAG_BYTES]);
        self.send_rtcp_index += 1;
        Ok(protected_length)
    }

    pub fn unprotect_rtcp(
        &mut self,
        packet: &mut [u8],
        packet_length: usize,
    ) -> Result<usize, SrtpError> {
        if packet_length < SRTCP_FIXED_RR_BYTES + SRTCP_INDEX_BYTES + AUTH_TAG_BYTES
            || packet_length > packet.len()
        {
            return Err(SrtpError::InvalidRtcpPacket);
        }
        let rtcp_end = packet_length - SRTCP_INDEX_BYTES - AUTH_TAG_BYTES;
        validate_receiver_report(packet, rtcp_end)?;
        let index_offset = rtcp_end;
        let index = u32::from_be_bytes(
            packet[index_offset..index_offset + SRTCP_INDEX_BYTES]
                .try_into()
                .expect("SRTCP index"),
        );
        let packet_index = index & 0x7fff_ffff;
        if !self.accept_rtcp_index(packet_index) {
            return Err(SrtpError::Replay);
        }
        let tag_offset = index_offset + SRTCP_INDEX_BYTES;
        let mut mac = HmacSha1::new_from_slice(&self.receive.rtcp_authentication_key)
            .expect("fixed SRTCP authentication key length");
        mac.update(&packet[..tag_offset]);
        let expected_tag = mac.finalize().into_bytes();
        let mut tag_difference = 0u8;
        for (expected, actual) in expected_tag[..AUTH_TAG_BYTES]
            .iter()
            .zip(&packet[tag_offset..packet_length])
        {
            tag_difference |= *expected ^ *actual;
        }
        if tag_difference != 0 {
            return Err(SrtpError::AuthenticationFailed);
        }

        if index & 0x8000_0000 != 0 {
            let ssrc = u32::from_be_bytes(packet[4..8].try_into().expect("RTCP SSRC"));
            self.crypt_rtcp_payload(
                &self.receive,
                packet,
                SRTCP_FIXED_RR_BYTES,
                rtcp_end,
                ssrc,
                packet_index,
            );
        }
        self.update_rtcp_replay_window(packet_index);
        Ok(rtcp_end)
    }

    pub const fn send_rollover_counter(&self) -> u32 {
        self.send_rollover_counter
    }

    fn next_send_rollover_counter(&self, sequence: u16) -> Result<u32, SrtpError> {
        let Some(last_sequence) = self.last_sent_sequence else {
            return Ok(0);
        };
        if sequence > last_sequence {
            return Ok(self.send_rollover_counter);
        }
        if last_sequence > 0x8000 && sequence < 0x8000 {
            return self
                .send_rollover_counter
                .checked_add(1)
                .ok_or(SrtpError::OutOfOrder);
        }
        Err(SrtpError::OutOfOrder)
    }

    fn guess_receive_index(&self, sequence: u16) -> Result<u64, SrtpError> {
        let Some(highest_index) = self.highest_received_index else {
            return Ok(u64::from(sequence));
        };
        let local_sequence = (highest_index & 0xffff) as u16;
        let rollover_counter = (highest_index >> 16) as u32;
        let estimated_rollover = if local_sequence < 0x8000 {
            if sequence > local_sequence && sequence - local_sequence > 0x8000 {
                rollover_counter.checked_sub(1).ok_or(SrtpError::Replay)?
            } else {
                rollover_counter
            }
        } else if local_sequence > sequence && local_sequence - sequence > 0x8000 {
            rollover_counter.checked_add(1).ok_or(SrtpError::Replay)?
        } else {
            rollover_counter
        };
        Ok((u64::from(estimated_rollover) << 16) | u64::from(sequence))
    }

    fn check_replay_window(&self, packet_index: u64) -> Result<(), SrtpError> {
        let Some(highest_index) = self.highest_received_index else {
            return Ok(());
        };
        if packet_index > highest_index {
            return Ok(());
        }
        let distance = highest_index - packet_index;
        if distance >= 64 || self.replay_window & (1u64 << distance) != 0 {
            return Err(SrtpError::Replay);
        }
        Ok(())
    }

    fn update_replay_window(&mut self, packet_index: u64) {
        match self.highest_received_index {
            None => {
                self.highest_received_index = Some(packet_index);
                self.replay_window = 1;
            }
            Some(highest_index) if packet_index > highest_index => {
                let distance = packet_index - highest_index;
                self.replay_window = if distance >= 64 {
                    1
                } else {
                    (self.replay_window << distance) | 1
                };
                self.highest_received_index = Some(packet_index);
            }
            Some(highest_index) => {
                self.replay_window |= 1u64 << (highest_index - packet_index);
            }
        }
    }

    fn accept_rtcp_index(&self, packet_index: u32) -> bool {
        match self.highest_received_rtcp_index {
            None => true,
            Some(highest) if packet_index > highest => true,
            Some(highest) => {
                let distance = highest - packet_index;
                distance < 64 && self.rtcp_replay_window & (1u64 << distance) == 0
            }
        }
    }

    fn update_rtcp_replay_window(&mut self, packet_index: u32) {
        match self.highest_received_rtcp_index {
            None => {
                self.rtcp_replay_window = 1;
                self.highest_received_rtcp_index = Some(packet_index);
            }
            Some(highest) if packet_index > highest => {
                let distance = packet_index - highest;
                self.rtcp_replay_window = if distance >= 64 {
                    1
                } else {
                    (self.rtcp_replay_window << distance) | 1
                };
                self.highest_received_rtcp_index = Some(packet_index);
            }
            Some(highest) => {
                self.rtcp_replay_window |= 1u64 << (highest - packet_index);
            }
        }
    }

    fn crypt_payload(
        &self,
        direction: &SrtpDirection,
        packet: &mut [u8],
        payload_start: usize,
        payload_end: usize,
        ssrc: u32,
        packet_index: u64,
    ) {
        let mut iv = [0u8; 16];
        iv[..14].copy_from_slice(&direction.session_salt);
        for (slot, byte) in iv[4..8].iter_mut().zip(ssrc.to_be_bytes()) {
            *slot ^= byte;
        }
        let index_bytes = packet_index.to_be_bytes();
        for index in 0..6 {
            iv[8 + index] ^= index_bytes[2 + index];
        }
        let mut cipher = Aes128Ctr::new((&direction.encryption_key).into(), (&iv).into());
        cipher.apply_keystream(&mut packet[payload_start..payload_end]);
    }

    fn crypt_rtcp_payload(
        &self,
        direction: &SrtpDirection,
        packet: &mut [u8],
        payload_start: usize,
        payload_end: usize,
        ssrc: u32,
        index: u32,
    ) {
        let mut iv = [0u8; 16];
        iv[..14].copy_from_slice(&direction.rtcp_session_salt);
        for (slot, byte) in iv[4..8].iter_mut().zip(ssrc.to_be_bytes()) {
            *slot ^= byte;
        }
        // RFC 3711 §4.1.1 uses index * 2^16: the 31-bit index occupies
        // bytes 10..14 of the 128-bit IV, with bytes 8..10 and 14..16 zero.
        for (slot, byte) in iv[10..14].iter_mut().zip(index.to_be_bytes()) {
            *slot ^= byte;
        }
        let mut cipher = Aes128Ctr::new((&direction.rtcp_encryption_key).into(), (&iv).into());
        cipher.apply_keystream(&mut packet[payload_start..payload_end]);
    }
}

impl SrtpDirection {
    fn from_master(master_key: &[u8; 16], master_salt: &[u8; 14]) -> Self {
        let (encryption_key, authentication_key, session_salt) =
            SrtpSession::derive_session_keys(master_key, master_salt);
        let rtcp_encryption_key = derive_key::<16>(master_key, master_salt, 0x03);
        let rtcp_authentication_key = derive_key::<AUTH_KEY_BYTES>(master_key, master_salt, 0x04);
        let rtcp_session_salt = derive_key::<14>(master_key, master_salt, 0x05);
        Self {
            encryption_key,
            authentication_key,
            session_salt,
            rtcp_encryption_key,
            rtcp_authentication_key,
            rtcp_session_salt,
        }
    }
}

fn validate_receiver_report(packet: &[u8], packet_length: usize) -> Result<(), SrtpError> {
    if packet_length < SRTCP_FIXED_RR_BYTES
        || packet_length > packet.len()
        || packet[0] >> 6 != 2
        || packet[1] != 201
    {
        return Err(SrtpError::InvalidRtcpPacket);
    }
    let first_packet_length = (usize::from(u16::from_be_bytes([packet[2], packet[3]]))
        .saturating_add(1))
    .checked_mul(4)
    .ok_or(SrtpError::InvalidRtcpPacket)?;
    let report_count = usize::from(packet[0] & 0x1f);
    let minimum_report_length = SRTCP_FIXED_RR_BYTES
        .checked_add(
            report_count
                .checked_mul(24)
                .ok_or(SrtpError::InvalidRtcpPacket)?,
        )
        .ok_or(SrtpError::InvalidRtcpPacket)?;
    if first_packet_length > packet_length || minimum_report_length > first_packet_length {
        return Err(SrtpError::InvalidRtcpPacket);
    }
    Ok(())
}

fn rtp_payload_offset(packet: &[u8], packet_length: usize) -> Option<usize> {
    if packet_length < RTP_FIXED_HEADER_BYTES || packet_length > packet.len() {
        return None;
    }
    let csrc_count = usize::from(packet[0] & 0x0f);
    let mut header_length = RTP_FIXED_HEADER_BYTES.checked_add(csrc_count.checked_mul(4)?)?;
    if header_length > packet_length {
        return None;
    }
    if packet[0] & 0x10 != 0 {
        let extension_header_end = header_length.checked_add(4)?;
        if extension_header_end > packet_length {
            return None;
        }
        let extension_words = usize::from(u16::from_be_bytes([
            packet[header_length + 2],
            packet[header_length + 3],
        ]));
        header_length = extension_header_end.checked_add(extension_words.checked_mul(4)?)?;
        if header_length > packet_length {
            return None;
        }
    }
    Some(header_length)
}

fn derive_key<const N: usize>(master_key: &[u8; 16], master_salt: &[u8; 14], label: u8) -> [u8; N] {
    let mut iv = [0u8; 16];
    iv[..14].copy_from_slice(master_salt);
    iv[7] ^= label;
    let mut key = [0u8; N];
    let mut cipher = Aes128Ctr::new(master_key.into(), (&iv).into());
    cipher.apply_keystream(&mut key);
    key
}

#[cfg(test)]
mod tests {
    use super::{SrtpDirection, SrtpError, SrtpSession};
    use crate::dtls::{DtlsRole, DtlsSrtpKeys};

    fn rtp_packet(sequence: u16, payload: &[u8], output: &mut [u8]) -> usize {
        output[0] = 0x80;
        output[1] = 111;
        output[2..4].copy_from_slice(&sequence.to_be_bytes());
        output[4..8].copy_from_slice(&0x0102_0304u32.to_be_bytes());
        output[8..12].copy_from_slice(&0x1122_3344u32.to_be_bytes());
        output[12..12 + payload.len()].copy_from_slice(payload);
        12 + payload.len()
    }

    #[test]
    fn derives_rfc3711_session_keys() {
        // Arrange
        let master_key = [
            0xE1, 0xF9, 0x7A, 0x0D, 0x3E, 0x01, 0x8B, 0xE0, 0xD6, 0x4F, 0xA3, 0x2C, 0x06, 0xDE,
            0x41, 0x39,
        ];
        let master_salt = [
            0x0E, 0xC6, 0x75, 0xAD, 0x49, 0x8A, 0xFE, 0xEB, 0xB6, 0x96, 0x0B, 0x3A, 0xAB, 0xE6,
        ];

        // Act
        let keys = SrtpSession::derive_session_keys(&master_key, &master_salt);

        // Assert
        assert_eq!(
            keys.0,
            [
                0xC6, 0x1E, 0x7A, 0x93, 0x74, 0x4F, 0x39, 0xEE, 0x10, 0x73, 0x4A, 0xFE, 0x3F, 0xF7,
                0xA0, 0x87,
            ]
        );
        assert_eq!(
            keys.1,
            [
                0xCE, 0xBE, 0x32, 0x1F, 0x6F, 0xF7, 0x71, 0x6B, 0x6F, 0xD4, 0xAB, 0x49, 0xAF, 0x25,
                0x6A, 0x15, 0x6D, 0x38, 0xBA, 0xA4,
            ]
        );
        assert_eq!(
            keys.2,
            [0x30, 0xCB, 0xBC, 0x08, 0x86, 0x3D, 0x8C, 0x85, 0xD4, 0x9D, 0xB3, 0x4A, 0x9A, 0xE1]
        );
    }

    #[test]
    fn protects_and_unprotects_rtp_payload() {
        // Arrange
        let key = [0x11; 16];
        let salt = [0x22; 14];
        let mut sender = SrtpSession::new(key, salt);
        let mut receiver = SrtpSession::new(key, salt);
        let mut packet = [0u8; 64];
        let packet_len = rtp_packet(9, b"pcm-opus", &mut packet);

        // Act
        let protected_len = sender
            .protect_rtp(&mut packet, packet_len)
            .expect("protect");
        let plain_len = receiver
            .unprotect_rtp(&mut packet, protected_len)
            .expect("unprotect");

        // Assert
        assert_eq!(plain_len, packet_len);
        assert_eq!(&packet[12..plain_len], b"pcm-opus");
    }

    #[test]
    fn protects_and_unprotects_srtcp_receiver_report() {
        // Arrange
        let mut sender = SrtpSession::new([0x11; 16], [0x22; 14]);
        let mut receiver = SrtpSession::new([0x11; 16], [0x22; 14]);
        let mut packet = [0; 64];
        packet[..8].copy_from_slice(&[0x80, 201, 0, 1, 0x01, 0x02, 0x03, 0x04]);

        // Act
        let protected_length = sender.protect_rtcp(&mut packet, 8).expect("protect");
        let plain_length = receiver
            .unprotect_rtcp(&mut packet, protected_length)
            .expect("unprotect");

        // Assert
        assert_eq!(plain_length, 8);
        assert_eq!(&packet[..8], &[0x80, 201, 0, 1, 0x01, 0x02, 0x03, 0x04]);
    }

    #[test]
    fn srtcp_ctr_iv_matches_rfc_3711_index_offset_known_answer() {
        // Arrange
        // RFC 3711 Appendix B.2 session key/salt; IV uses its §4.1.1 SRTCP index formula.
        // For SSRC=0 and index=1, salt<<16 XOR index<<16 is F0F1...FCFC0000.
        let direction = SrtpDirection {
            encryption_key: [0; 16],
            authentication_key: [0; 20],
            session_salt: [0; 14],
            rtcp_encryption_key: [
                0x2b, 0x7e, 0x15, 0x16, 0x28, 0xae, 0xd2, 0xa6, 0xab, 0xf7, 0x15, 0x88, 0x09, 0xcf,
                0x4f, 0x3c,
            ],
            rtcp_authentication_key: [0; 20],
            rtcp_session_salt: [
                0xf0, 0xf1, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8, 0xf9, 0xfa, 0xfb, 0xfc, 0xfd,
            ],
        };
        let session = SrtpSession::new([0; 16], [0; 14]);
        let mut packet = [0; 32];
        packet[..8].copy_from_slice(&[0x81, 201, 0, 7, 0, 0, 0, 0]);
        for (index, byte) in packet[8..32].iter_mut().enumerate() {
            *byte = index as u8;
        }

        // Act
        session.crypt_rtcp_payload(&direction, &mut packet, 8, 32, 0, 1);

        // Assert
        assert_eq!(
            &packet[8..32],
            &[
                0x41, 0xd3, 0xba, 0xb1, 0x1e, 0x1f, 0xe0, 0x7a, 0x77, 0xdf, 0xca, 0x06, 0x16, 0xf7,
                0xe1, 0x75, 0x30, 0x69, 0xe1, 0xcd, 0x0a, 0x31, 0x52, 0x34,
            ]
        );
    }

    #[test]
    fn leaves_csrc_and_header_extension_clear() {
        // Arrange
        let key = [0x13; 16];
        let salt = [0x24; 14];
        let mut sender = SrtpSession::new(key, salt);
        let mut receiver = SrtpSession::new(key, salt);
        let mut packet = [0u8; 64];
        packet[0] = 0x91; // RTP v2, extension bit, one CSRC
        packet[1] = 111;
        packet[2..4].copy_from_slice(&1u16.to_be_bytes());
        packet[4..8].copy_from_slice(&0x0102_0304u32.to_be_bytes());
        packet[8..12].copy_from_slice(&0x1122_3344u32.to_be_bytes());
        packet[12..16].copy_from_slice(&0x5566_7788u32.to_be_bytes());
        packet[16..18].copy_from_slice(&0xBEDEu16.to_be_bytes());
        packet[18..20].copy_from_slice(&1u16.to_be_bytes());
        packet[20..24].copy_from_slice(&[0x10, 0xAA, 0, 0]);
        packet[24..30].copy_from_slice(b"audio!");
        let packet_len = 30;
        let clear_header = packet[..24].to_vec();

        // Act
        let protected_len = sender
            .protect_rtp(&mut packet, packet_len)
            .expect("protect");
        let protected_header = packet[..24].to_vec();
        let protected_payload = packet[24..packet_len].to_vec();
        let plain_len = receiver
            .unprotect_rtp(&mut packet, protected_len)
            .expect("unprotect");

        // Assert
        assert_eq!(protected_header.as_slice(), clear_header.as_slice());
        assert_eq!(&packet[..24], clear_header.as_slice());
        assert_ne!(&protected_payload[..], b"audio!");
        assert_eq!(&packet[24..plain_len], b"audio!");
    }

    #[test]
    fn rejects_tampered_packet_without_decrypting_or_advancing_replay_window() {
        // Arrange
        let key = [0x33; 16];
        let salt = [0x44; 14];
        let mut sender = SrtpSession::new(key, salt);
        let mut receiver = SrtpSession::new(key, salt);
        let mut packet = [0u8; 64];
        let packet_len = rtp_packet(20, b"authenticated", &mut packet);
        let protected_len = sender
            .protect_rtp(&mut packet, packet_len)
            .expect("protect");
        packet[12] ^= 1;

        // Act
        let result = receiver.unprotect_rtp(&mut packet, protected_len);

        // Assert
        assert_eq!(result, Err(SrtpError::AuthenticationFailed));
    }

    #[test]
    fn rejects_replayed_rtp_packet() {
        // Arrange
        let key = [0x55; 16];
        let salt = [0x66; 14];
        let mut sender = SrtpSession::new(key, salt);
        let mut receiver = SrtpSession::new(key, salt);
        let mut packet = [0u8; 64];
        let packet_len = rtp_packet(7, b"replay", &mut packet);
        let protected_len = sender
            .protect_rtp(&mut packet, packet_len)
            .expect("protect");
        receiver
            .unprotect_rtp(&mut packet, protected_len)
            .expect("first packet");

        // Act
        let result = receiver.unprotect_rtp(&mut packet, protected_len);

        // Assert
        assert_eq!(result, Err(SrtpError::Replay));
    }

    #[test]
    fn sender_increments_rollover_counter_at_sequence_wrap() {
        // Arrange
        let mut sender = SrtpSession::new([0x77; 16], [0x88; 14]);
        let mut packet = [0u8; 64];
        let before_wrap_len = rtp_packet(u16::MAX, b"a", &mut packet);
        sender
            .protect_rtp(&mut packet, before_wrap_len)
            .expect("protect before wrap");
        let after_wrap_len = rtp_packet(0, b"b", &mut packet);

        // Act
        let result = sender.protect_rtp(&mut packet, after_wrap_len);

        // Assert
        assert!(result.is_ok());
        assert_eq!(sender.send_rollover_counter(), 1);
    }

    #[test]
    fn dtls_role_selects_matching_directional_keys() {
        // Arrange
        let keys = DtlsSrtpKeys {
            client_master_key: [0x11; 16],
            server_master_key: [0x22; 16],
            client_master_salt: [0x33; 14],
            server_master_salt: [0x44; 14],
        };
        let mut client = SrtpSession::from_dtls_keys(keys, DtlsRole::Client);
        let mut server = SrtpSession::from_dtls_keys(keys, DtlsRole::Server);
        let mut client_packet = [0; 64];
        let mut server_packet = [0; 64];
        let client_len = rtp_packet(1, b"client", &mut client_packet);
        let server_len = rtp_packet(1, b"server", &mut server_packet);

        // Act
        let client_protected = client
            .protect_rtp(&mut client_packet, client_len)
            .expect("client protect");
        let server_protected = server
            .protect_rtp(&mut server_packet, server_len)
            .expect("server protect");
        let client_plain = server
            .unprotect_rtp(&mut client_packet, client_protected)
            .expect("server receives client packet");
        let server_plain = client
            .unprotect_rtp(&mut server_packet, server_protected)
            .expect("client receives server packet");

        // Assert
        assert_eq!(&client_packet[12..client_plain], b"client");
        assert_eq!(&server_packet[12..server_plain], b"server");
    }

    #[test]
    fn accepts_reordered_packets_inside_replay_window() {
        // Arrange
        let key = [0x91; 16];
        let salt = [0x92; 14];
        let mut sender = SrtpSession::new(key, salt);
        let mut receiver = SrtpSession::new(key, salt);
        let mut packets = [[0u8; 64]; 3];
        let mut lengths = [0; 3];
        for (index, sequence) in [10, 11, 12].into_iter().enumerate() {
            let length = rtp_packet(sequence, b"reorder", &mut packets[index]);
            lengths[index] = sender
                .protect_rtp(&mut packets[index], length)
                .expect("protect");
        }
        receiver
            .unprotect_rtp(&mut packets[0], lengths[0])
            .expect("first");
        receiver
            .unprotect_rtp(&mut packets[2], lengths[2])
            .expect("third arrives before second");

        // Act
        let result = receiver.unprotect_rtp(&mut packets[1], lengths[1]);

        // Assert
        assert_eq!(result, Ok(lengths[1] - 10));
        assert_eq!(
            receiver.unprotect_rtp(&mut packets[1], lengths[1]),
            Err(SrtpError::Replay)
        );
    }

    #[test]
    fn rejects_packets_older_than_replay_window() {
        // Arrange
        let key = [0xA1; 16];
        let salt = [0xA2; 14];
        let mut sender = SrtpSession::new(key, salt);
        let mut receiver = SrtpSession::new(key, salt);
        let mut oldest = [0u8; 64];
        let oldest_len = rtp_packet(1, b"old", &mut oldest);
        let oldest_protected = sender
            .protect_rtp(&mut oldest, oldest_len)
            .expect("protect oldest");
        receiver
            .unprotect_rtp(&mut oldest, oldest_protected)
            .expect("accept oldest initially");
        let mut newest = [0u8; 64];
        let newest_len = rtp_packet(66, b"new", &mut newest);
        let newest_protected = sender
            .protect_rtp(&mut newest, newest_len)
            .expect("protect newest");
        receiver
            .unprotect_rtp(&mut newest, newest_protected)
            .expect("advance beyond replay window");

        // Act
        let result = receiver.unprotect_rtp(&mut oldest, oldest_protected);

        // Assert
        assert_eq!(result, Err(SrtpError::Replay));
    }

    #[test]
    fn receiver_guesses_previous_rollover_for_late_pre_wrap_packet() {
        // Arrange
        let key = [0xB1; 16];
        let salt = [0xB2; 14];
        let mut sender = SrtpSession::new(key, salt);
        let mut receiver = SrtpSession::new(key, salt);
        let mut initial = [0u8; 64];
        let initial_len = rtp_packet(u16::MAX - 1, b"initial", &mut initial);
        let initial_protected = sender
            .protect_rtp(&mut initial, initial_len)
            .expect("protect initial");
        let mut before_wrap = [0u8; 64];
        let before_wrap_len = rtp_packet(u16::MAX, b"before", &mut before_wrap);
        let before_wrap_protected = sender
            .protect_rtp(&mut before_wrap, before_wrap_len)
            .expect("protect before wrap");
        let mut after_wrap = [0u8; 64];
        let after_wrap_len = rtp_packet(0, b"after", &mut after_wrap);
        let after_wrap_protected = sender
            .protect_rtp(&mut after_wrap, after_wrap_len)
            .expect("protect after wrap");
        receiver
            .unprotect_rtp(&mut initial, initial_protected)
            .expect("initialize receive rollover counter");
        receiver
            .unprotect_rtp(&mut after_wrap, after_wrap_protected)
            .expect("receive post-wrap packet first");

        // Act
        let result = receiver.unprotect_rtp(&mut before_wrap, before_wrap_protected);

        // Assert
        assert_eq!(result, Ok(before_wrap_len));
        assert_eq!(&before_wrap[12..before_wrap_len], b"before");
    }
}
