//! ICE-Lite STUN Binding Request authentication and response construction.

use crc32fast::Hasher as Crc32Hasher;
use embassy_net::{IpAddress, IpEndpoint};
use heapless::String;
use hmac::{Hmac, Mac};
use sha1::Sha1;

const STUN_MAGIC_COOKIE: u32 = 0x2112_A442;
const STUN_BINDING_REQUEST: u16 = 0x0001;
const STUN_BINDING_SUCCESS_RESPONSE: u16 = 0x0101;
const ATTR_USERNAME: u16 = 0x0006;
const ATTR_PRIORITY: u16 = 0x0024;
const ATTR_USE_CANDIDATE: u16 = 0x0025;
const ATTR_XOR_MAPPED_ADDRESS: u16 = 0x0020;
const ATTR_MESSAGE_INTEGRITY: u16 = 0x0008;
const ATTR_FINGERPRINT: u16 = 0x8028;
const ATTR_ICE_CONTROLLED: u16 = 0x8029;
const ATTR_ICE_CONTROLLING: u16 = 0x802a;
const STUN_FINGERPRINT_XOR: u32 = 0x5354_554e;
type HmacSha1 = Hmac<Sha1>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IceError {
    InvalidLocalCredentials,
}

pub struct IceLiteAgent {
    local_ufrag: String<256>,
    remote_ufrag: String<256>,
    local_pwd: String<256>,
    verified_peer: Option<IpEndpoint>,
}

impl IceLiteAgent {
    pub fn new(local_ufrag: &str, remote_ufrag: &str, local_pwd: &str) -> Result<Self, IceError> {
        if !(4..=256).contains(&local_ufrag.len())
            || !(4..=256).contains(&remote_ufrag.len())
            || !(22..=256).contains(&local_pwd.len())
            || !local_ufrag.bytes().all(is_ice_char)
            || !remote_ufrag.bytes().all(is_ice_char)
            || !local_pwd.bytes().all(is_ice_char)
        {
            return Err(IceError::InvalidLocalCredentials);
        }
        let mut ufrag = String::new();
        let mut peer_ufrag = String::new();
        let mut pwd = String::new();
        ufrag
            .push_str(local_ufrag)
            .map_err(|_| IceError::InvalidLocalCredentials)?;
        peer_ufrag
            .push_str(remote_ufrag)
            .map_err(|_| IceError::InvalidLocalCredentials)?;
        pwd.push_str(local_pwd)
            .map_err(|_| IceError::InvalidLocalCredentials)?;
        Ok(Self {
            local_ufrag: ufrag,
            remote_ufrag: peer_ufrag,
            local_pwd: pwd,
            verified_peer: None,
        })
    }

    pub const fn verified_peer_addr(&self) -> Option<IpEndpoint> {
        self.verified_peer
    }

    pub fn handle_stun_binding_request(
        &mut self,
        packet: &[u8],
        remote: IpEndpoint,
        output: &mut [u8; 128],
    ) -> Option<usize> {
        let (username, integrity_offset, integrity, fingerprint_offset, fingerprint, nominated) =
            parse_binding_request(packet)?;
        if !username_targets_local(
            username,
            self.local_ufrag.as_bytes(),
            self.remote_ufrag.as_bytes(),
        ) || !verify_message_integrity(
            packet,
            integrity_offset,
            integrity,
            self.local_pwd.as_bytes(),
        ) || !verify_fingerprint(packet, fingerprint_offset, fingerprint)
        {
            return None;
        }
        if nominated && self.verified_peer.is_some() && self.verified_peer != Some(remote) {
            return None;
        }

        let response_length =
            write_binding_success(output, &packet[8..20], remote, self.local_pwd.as_bytes())?;
        if nominated && self.verified_peer.is_none() {
            self.verified_peer = Some(remote);
        }
        Some(response_length)
    }
}

fn parse_binding_request(packet: &[u8]) -> Option<(&[u8], usize, &[u8], usize, &[u8], bool)> {
    if packet.len() < 20
        || packet[0] & 0xc0 != 0
        || u16::from_be_bytes([packet[0], packet[1]]) != STUN_BINDING_REQUEST
        || u32::from_be_bytes(packet[4..8].try_into().ok()?) != STUN_MAGIC_COOKIE
        || usize::from(u16::from_be_bytes([packet[2], packet[3]])) != packet.len() - 20
        || (packet.len() - 20) % 4 != 0
    {
        return None;
    }

    let mut username = None;
    let mut priority = None;
    let mut controlling = false;
    let mut nominated = false;
    let mut integrity = None;
    let mut fingerprint = None;
    let mut offset = 20;
    while offset < packet.len() {
        let attribute_header_end = offset.checked_add(4)?;
        if attribute_header_end > packet.len() {
            return None;
        }
        let attribute_type = u16::from_be_bytes([packet[offset], packet[offset + 1]]);
        let attribute_length =
            usize::from(u16::from_be_bytes([packet[offset + 2], packet[offset + 3]]));
        let value_end = attribute_header_end.checked_add(attribute_length)?;
        let padded_end = attribute_header_end.checked_add((attribute_length + 3) & !3)?;
        if padded_end > packet.len() {
            return None;
        }
        let value = &packet[attribute_header_end..value_end];
        match attribute_type {
            ATTR_USERNAME => {
                if username.is_some() || integrity.is_some() || value.is_empty() {
                    return None;
                }
                username = Some(value);
            }
            ATTR_PRIORITY => {
                if priority.is_some() || attribute_length != 4 {
                    return None;
                }
                let value = u32::from_be_bytes(value.try_into().ok()?);
                if value == 0 || value > 0x7fff_ffff {
                    return None;
                }
                priority = Some(value);
            }
            ATTR_ICE_CONTROLLING => {
                if controlling || attribute_length != 8 {
                    return None;
                }
                controlling = true;
            }
            ATTR_ICE_CONTROLLED => {
                // This ICE-Lite endpoint is controlled; a controlling peer must
                // identify itself with ICE-CONTROLLING in each connectivity check.
                if attribute_length != 8 {
                    return None;
                }
                return None;
            }
            ATTR_USE_CANDIDATE => {
                if nominated || attribute_length != 0 {
                    return None;
                }
                nominated = true;
            }
            ATTR_MESSAGE_INTEGRITY => {
                if integrity.is_some()
                    || fingerprint.is_some()
                    || attribute_length != 20
                    || username.is_none()
                {
                    return None;
                }
                integrity = Some((offset, value));
            }
            ATTR_FINGERPRINT => {
                if fingerprint.is_some() || attribute_length != 4 || padded_end != packet.len() {
                    return None;
                }
                fingerprint = Some((offset, value));
            }
            _ if fingerprint.is_some() => return None,
            _ => {}
        }
        offset = padded_end;
    }
    let (integrity_offset, integrity) = integrity?;
    let (fingerprint_offset, fingerprint) = fingerprint?;
    if fingerprint_offset < integrity_offset + 24 || priority.is_none() || !controlling {
        return None;
    }
    Some((
        username?,
        integrity_offset,
        integrity,
        fingerprint_offset,
        fingerprint,
        nominated,
    ))
}

fn username_targets_local(username: &[u8], local_ufrag: &[u8], remote_ufrag: &[u8]) -> bool {
    username.len() == local_ufrag.len() + 1 + remote_ufrag.len()
        && username.starts_with(local_ufrag)
        && username[local_ufrag.len()] == b':'
        && &username[local_ufrag.len() + 1..] == remote_ufrag
}

const fn is_ice_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'+' || byte == b'/'
}

fn verify_message_integrity(
    packet: &[u8],
    integrity_offset: usize,
    expected_integrity: &[u8],
    password: &[u8],
) -> bool {
    let Some(integrity_end) = integrity_offset.checked_add(24) else {
        return false;
    };
    if integrity_end > packet.len() || expected_integrity.len() != 20 {
        return false;
    }
    let mut header = [0u8; 20];
    header.copy_from_slice(&packet[..20]);
    header[2..4].copy_from_slice(&((integrity_end - 20) as u16).to_be_bytes());
    let Ok(mut mac) = HmacSha1::new_from_slice(password) else {
        return false;
    };
    mac.update(&header);
    mac.update(&packet[20..integrity_offset]);
    mac.verify_slice(expected_integrity).is_ok()
}

fn verify_fingerprint(packet: &[u8], fingerprint_offset: usize, expected: &[u8]) -> bool {
    if expected.len() != 4 || fingerprint_offset > packet.len() {
        return false;
    }
    let mut crc = Crc32Hasher::new();
    crc.update(&packet[..fingerprint_offset]);
    let actual = crc.finalize() ^ STUN_FINGERPRINT_XOR;
    expected == actual.to_be_bytes()
}

fn write_binding_success(
    output: &mut [u8; 128],
    transaction_id: &[u8],
    remote: IpEndpoint,
    password: &[u8],
) -> Option<usize> {
    if transaction_id.len() != 12 {
        return None;
    }
    output.fill(0);
    output[0..2].copy_from_slice(&STUN_BINDING_SUCCESS_RESPONSE.to_be_bytes());
    output[4..8].copy_from_slice(&STUN_MAGIC_COOKIE.to_be_bytes());
    output[8..20].copy_from_slice(transaction_id);

    let (address_family, address_length) = match remote.addr {
        IpAddress::Ipv4(address) => {
            let octets = address.octets();
            for index in 0..4 {
                output[28 + index] = octets[index] ^ STUN_MAGIC_COOKIE.to_be_bytes()[index];
            }
            (0x01, 4)
        }
        IpAddress::Ipv6(address) => {
            let octets = address.octets();
            let mut mask = [0u8; 16];
            mask[..4].copy_from_slice(&STUN_MAGIC_COOKIE.to_be_bytes());
            mask[4..].copy_from_slice(transaction_id);
            for index in 0..16 {
                output[28 + index] = octets[index] ^ mask[index];
            }
            (0x02, 16)
        }
    };
    let address_value_length = 4 + address_length;
    let integrity_offset = 20 + 4 + address_value_length;
    let fingerprint_offset = integrity_offset + 24;
    let response_length = fingerprint_offset + 8;
    let integrity_message_length = integrity_offset + 24 - 20;
    let full_message_length = response_length - 20;
    output[2..4].copy_from_slice(&(integrity_message_length as u16).to_be_bytes());
    output[20..22].copy_from_slice(&ATTR_XOR_MAPPED_ADDRESS.to_be_bytes());
    output[22..24].copy_from_slice(&(address_value_length as u16).to_be_bytes());
    output[24] = 0;
    output[25] = address_family;
    output[26..28]
        .copy_from_slice(&(remote.port ^ ((STUN_MAGIC_COOKIE >> 16) as u16)).to_be_bytes());
    output[integrity_offset..integrity_offset + 2]
        .copy_from_slice(&ATTR_MESSAGE_INTEGRITY.to_be_bytes());
    output[integrity_offset + 2..integrity_offset + 4].copy_from_slice(&20u16.to_be_bytes());
    let mut mac_header = [0u8; 20];
    mac_header.copy_from_slice(&output[..20]);
    mac_header[2..4].copy_from_slice(&(integrity_message_length as u16).to_be_bytes());
    let mut mac = HmacSha1::new_from_slice(password).ok()?;
    mac.update(&mac_header);
    mac.update(&output[20..integrity_offset]);
    output[integrity_offset + 4..integrity_offset + 24]
        .copy_from_slice(&mac.finalize().into_bytes());

    output[2..4].copy_from_slice(&(full_message_length as u16).to_be_bytes());
    output[fingerprint_offset..fingerprint_offset + 2]
        .copy_from_slice(&ATTR_FINGERPRINT.to_be_bytes());
    output[fingerprint_offset + 2..fingerprint_offset + 4].copy_from_slice(&4u16.to_be_bytes());
    let mut crc = Crc32Hasher::new();
    crc.update(&output[..fingerprint_offset]);
    output[fingerprint_offset + 4..response_length]
        .copy_from_slice(&(crc.finalize() ^ STUN_FINGERPRINT_XOR).to_be_bytes());
    Some(response_length)
}

#[cfg(test)]
mod tests {
    use super::IceLiteAgent;
    use crc32fast::Hasher as Crc32Hasher;
    use embassy_net::{IpAddress, IpEndpoint, Ipv4Address, Ipv6Address};
    use hmac::{Hmac, Mac};
    use sha1::Sha1;
    use std::vec;
    use std::vec::Vec;

    type HmacSha1 = Hmac<Sha1>;

    fn binding_request(username: &[u8], password: &[u8], corrupt_integrity: bool) -> Vec<u8> {
        binding_request_with_ice_attributes(
            username,
            password,
            corrupt_integrity,
            true,
            true,
            false,
            true,
        )
    }

    fn binding_request_with_ice_attributes(
        username: &[u8],
        password: &[u8],
        corrupt_integrity: bool,
        include_priority: bool,
        include_controlling: bool,
        include_controlled: bool,
        nominated: bool,
    ) -> Vec<u8> {
        let mut packet = vec![0u8; 20];
        packet[0..2].copy_from_slice(&0x0001u16.to_be_bytes());
        packet[4..8].copy_from_slice(&0x2112_A442u32.to_be_bytes());
        packet[8..20].copy_from_slice(b"transaction1");

        append_attribute(&mut packet, 0x0006, username);
        if include_priority {
            append_attribute(&mut packet, 0x0024, &0x6e00_01ffu32.to_be_bytes());
        }
        if include_controlling {
            append_attribute(&mut packet, 0x802a, &0x0102_0304_0506_0708u64.to_be_bytes());
        }
        if include_controlled {
            append_attribute(&mut packet, 0x8029, &0x0102_0304_0506_0708u64.to_be_bytes());
        }
        if nominated {
            append_attribute(&mut packet, 0x0025, &[]);
        }

        let mi_offset = packet.len();
        append_attribute(&mut packet, 0x0008, &[0; 20]);
        let fp_offset = packet.len();
        append_attribute(&mut packet, 0x8028, &[0; 4]);
        let message_length = (packet.len() - 20) as u16;
        packet[2..4].copy_from_slice(&message_length.to_be_bytes());

        let mut hmac_header = [0u8; 20];
        hmac_header.copy_from_slice(&packet[..20]);
        hmac_header[2..4].copy_from_slice(&((mi_offset + 24 - 20) as u16).to_be_bytes());
        let mut mac = HmacSha1::new_from_slice(password).expect("HMAC key");
        mac.update(&hmac_header);
        mac.update(&packet[20..mi_offset]);
        let mut tag = mac.finalize().into_bytes();
        if corrupt_integrity {
            tag[0] ^= 0x80;
        }
        packet[mi_offset + 4..mi_offset + 24].copy_from_slice(&tag);
        let mut crc = Crc32Hasher::new();
        crc.update(&packet[..fp_offset]);
        packet[fp_offset + 4..fp_offset + 8]
            .copy_from_slice(&(crc.finalize() ^ 0x5354_554e).to_be_bytes());
        packet
    }

    fn append_attribute(packet: &mut Vec<u8>, attribute_type: u16, value: &[u8]) {
        packet.extend_from_slice(&attribute_type.to_be_bytes());
        packet.extend_from_slice(&(value.len() as u16).to_be_bytes());
        packet.extend_from_slice(value);
        while packet.len() % 4 != 0 {
            packet.push(0);
        }
    }

    #[test]
    fn authenticated_binding_request_verifies_peer_and_returns_xor_mapped_address() {
        // Arrange
        let mut agent =
            IceLiteAgent::new("loca", "peer", "localpassword0123456789").expect("agent");
        let remote = IpEndpoint::new(IpAddress::Ipv4(Ipv4Address::new(192, 0, 2, 44)), 54_321);
        let packet = binding_request(b"loca:peer", b"localpassword0123456789", false);
        let mut response = [0u8; 128];

        // Act
        let response_len = agent.handle_stun_binding_request(&packet, remote, &mut response);

        // Assert
        assert_eq!(agent.verified_peer_addr(), Some(remote));
        assert_eq!(response_len, Some(64));
        assert_eq!(&response[8..20], &packet[8..20]);
        assert_eq!(u16::from_be_bytes([response[0], response[1]]), 0x0101);
        assert_eq!(u16::from_be_bytes([response[24], response[25]]), 0x0001);
        assert_eq!(
            u16::from_be_bytes([response[26], response[27]]),
            54_321 ^ 0x2112
        );
        assert!(super::verify_message_integrity(
            &response,
            32,
            &response[36..56],
            b"localpassword0123456789"
        ));
        assert!(super::verify_fingerprint(&response, 56, &response[60..64]));
    }

    #[test]
    fn invalid_message_integrity_never_installs_peer() {
        // Arrange
        let mut agent =
            IceLiteAgent::new("loca", "peer", "localpassword0123456789").expect("agent");
        let remote = IpEndpoint::new(IpAddress::Ipv4(Ipv4Address::new(198, 51, 100, 9)), 9_999);
        let packet = binding_request(b"loca:peer", b"localpassword0123456789", true);
        let mut response = [0u8; 128];

        // Act
        let response_len = agent.handle_stun_binding_request(&packet, remote, &mut response);

        // Assert
        assert_eq!(response_len, None);
        assert_eq!(agent.verified_peer_addr(), None);
    }

    #[test]
    fn username_must_target_local_ice_ufrag_before_peer_is_installed() {
        // Arrange
        let mut agent =
            IceLiteAgent::new("loca", "peer", "localpassword0123456789").expect("agent");
        let remote = IpEndpoint::new(IpAddress::Ipv4(Ipv4Address::new(203, 0, 113, 4)), 40_000);
        let packet = binding_request(b"other:peer", b"localpassword0123456789", false);
        let mut response = [0u8; 128];

        // Act
        let response_len = agent.handle_stun_binding_request(&packet, remote, &mut response);

        // Assert
        assert_eq!(response_len, None);
        assert_eq!(agent.verified_peer_addr(), None);
    }

    #[test]
    fn username_must_match_expected_remote_ufrag() {
        // Arrange
        let mut agent =
            IceLiteAgent::new("loca", "peer", "localpassword0123456789").expect("agent");
        let remote = IpEndpoint::new(IpAddress::Ipv4(Ipv4Address::new(203, 0, 113, 4)), 40_000);
        let packet = binding_request(b"loca:other", b"localpassword0123456789", false);
        let mut response = [0u8; 128];

        // Act
        let response_len = agent.handle_stun_binding_request(&packet, remote, &mut response);

        // Assert
        assert_eq!(response_len, None);
        assert_eq!(agent.verified_peer_addr(), None);
    }

    #[test]
    fn ipv6_success_response_xors_full_mapped_address() {
        // Arrange
        let mut agent =
            IceLiteAgent::new("loca", "peer", "localpassword0123456789").expect("agent");
        let remote = IpEndpoint::new(
            IpAddress::Ipv6(Ipv6Address::new(0x2001, 0xdb8, 1, 2, 3, 4, 5, 6)),
            12_345,
        );
        let packet = binding_request(b"loca:peer", b"localpassword0123456789", false);
        let mut response = [0u8; 128];

        // Act
        let response_len = agent.handle_stun_binding_request(&packet, remote, &mut response);

        // Assert
        assert_eq!(response_len, Some(76));
        assert_eq!(agent.verified_peer_addr(), Some(remote));
        assert_eq!(&response[24..26], &[0, 2]);
        assert_eq!(
            u16::from_be_bytes([response[26], response[27]]),
            12_345 ^ 0x2112
        );
        assert!(super::verify_message_integrity(
            &response,
            44,
            &response[48..68],
            b"localpassword0123456789"
        ));
        assert!(super::verify_fingerprint(&response, 68, &response[72..76]));
    }

    #[test]
    fn invalid_fingerprint_never_installs_peer() {
        // Arrange
        let mut agent =
            IceLiteAgent::new("loca", "peer", "localpassword0123456789").expect("agent");
        let remote = IpEndpoint::new(IpAddress::Ipv4(Ipv4Address::new(192, 0, 2, 1)), 30_000);
        let mut packet = binding_request(b"loca:peer", b"localpassword0123456789", false);
        let last = packet.len() - 1;
        packet[last] ^= 1;
        let mut response = [0u8; 128];

        // Act
        let response_len = agent.handle_stun_binding_request(&packet, remote, &mut response);

        // Assert
        assert_eq!(response_len, None);
        assert_eq!(agent.verified_peer_addr(), None);
    }

    #[test]
    fn valid_non_nominated_check_gets_response_without_selecting_peer() {
        // Arrange
        let mut agent =
            IceLiteAgent::new("loca", "peer", "localpassword0123456789").expect("agent");
        let remote = IpEndpoint::new(IpAddress::Ipv4(Ipv4Address::new(192, 0, 2, 8)), 20_000);
        let packet = binding_request_with_ice_attributes(
            b"loca:peer",
            b"localpassword0123456789",
            false,
            true,
            true,
            false,
            false,
        );
        let mut response = [0u8; 128];

        // Act
        let result = agent.handle_stun_binding_request(&packet, remote, &mut response);

        // Assert
        assert_eq!(result, Some(64));
        assert_eq!(agent.verified_peer_addr(), None);
    }

    #[test]
    fn nomination_selects_peer_once_and_later_checks_cannot_replace_it() {
        // Arrange
        let mut agent =
            IceLiteAgent::new("loca", "peer", "localpassword0123456789").expect("agent");
        let selected = IpEndpoint::new(IpAddress::Ipv4(Ipv4Address::new(192, 0, 2, 8)), 20_000);
        let other = IpEndpoint::new(IpAddress::Ipv4(Ipv4Address::new(192, 0, 2, 9)), 20_001);
        let nomination = binding_request(b"loca:peer", b"localpassword0123456789", false);
        let non_nominated = binding_request_with_ice_attributes(
            b"loca:peer",
            b"localpassword0123456789",
            false,
            true,
            true,
            false,
            false,
        );
        let later_nomination = binding_request(b"loca:peer", b"localpassword0123456789", false);
        let mut response = [0u8; 128];

        // Act
        assert!(agent
            .handle_stun_binding_request(&nomination, selected, &mut response)
            .is_some());
        let response_to_non_nomination =
            agent.handle_stun_binding_request(&non_nominated, other, &mut response);
        let response_to_renomination =
            agent.handle_stun_binding_request(&later_nomination, other, &mut response);

        // Assert
        assert!(response_to_non_nomination.is_some());
        assert_eq!(response_to_renomination, None);
        assert_eq!(agent.verified_peer_addr(), Some(selected));
    }

    #[test]
    fn missing_or_conflicting_ice_role_attributes_do_not_select_peer() {
        // Arrange
        let mut malformed_priority =
            binding_request(b"loca:peer", b"localpassword0123456789", false);
        malformed_priority[38..40].copy_from_slice(&3u16.to_be_bytes());
        let mut malformed_controlling =
            binding_request(b"loca:peer", b"localpassword0123456789", false);
        malformed_controlling[46..48].copy_from_slice(&7u16.to_be_bytes());
        let cases = [
            binding_request_with_ice_attributes(
                b"loca:peer",
                b"localpassword0123456789",
                false,
                false,
                true,
                false,
                true,
            ),
            binding_request_with_ice_attributes(
                b"loca:peer",
                b"localpassword0123456789",
                false,
                true,
                false,
                false,
                true,
            ),
            binding_request_with_ice_attributes(
                b"loca:peer",
                b"localpassword0123456789",
                false,
                true,
                true,
                true,
                true,
            ),
            malformed_priority,
            malformed_controlling,
        ];

        // Act
        let results: Vec<_> = cases
            .iter()
            .map(|packet| {
                let mut agent =
                    IceLiteAgent::new("loca", "peer", "localpassword0123456789").expect("agent");
                let remote =
                    IpEndpoint::new(IpAddress::Ipv4(Ipv4Address::new(192, 0, 2, 10)), 20_002);
                let mut response = [0u8; 128];
                let result = agent.handle_stun_binding_request(packet, remote, &mut response);
                (result, agent.verified_peer_addr())
            })
            .collect();

        // Assert
        assert!(results
            .iter()
            .all(|(response, peer)| response.is_none() && peer.is_none()));
    }
}
