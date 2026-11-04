//! Zero-allocation SDP Offer parsing and compact RFC 7587 Opus offer/answer writing.

use core::fmt::Write;

pub const LOCAL_OFFER_MID: &str = "0";
pub const LOCAL_OFFER_OPUS_PAYLOAD_TYPE: u8 = 111;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SdpError {
    MissingAudioSection,
    MissingIceCredentials,
    MissingFingerprint,
    MissingOpusCodec,
    MissingDtlsSetup,
    MissingRtcpMux,
    MissingMid,
    InvalidAttribute,
    BufferTooSmall,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DtlsSetupRole {
    ActPass,
    Active,
    Passive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SdpMediaDirection {
    SendRecv,
    SendOnly,
    RecvOnly,
    Inactive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParsedSdpOffer<'a> {
    pub mid: &'a str,
    pub ice_ufrag: &'a str,
    pub ice_pwd: &'a str,
    pub sha256_fingerprint: [u8; 32],
    pub setup: DtlsSetupRole,
    pub opus_payload_type: u8,
    pub direction: SdpMediaDirection,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalIceCredentials<'a> {
    pub ice_ufrag: &'a str,
    pub ice_pwd: &'a str,
}

#[derive(Clone, Copy, Debug)]
pub struct LocalSdpParams<'a> {
    pub ipv4: [u8; 4],
    pub ipv6: [u16; 8],
    pub port: u16,
    pub ice_ufrag: &'a str,
    pub ice_pwd: &'a str,
    pub sha256_fingerprint: [u8; 32],
    pub ssrc: u32,
}

impl LocalSdpParams<'_> {
    pub const fn ice_credentials(&self) -> LocalIceCredentials<'_> {
        LocalIceCredentials {
            ice_ufrag: self.ice_ufrag,
            ice_pwd: self.ice_pwd,
        }
    }
}

pub fn parse_sdp_offer(sdp: &str) -> Result<ParsedSdpOffer<'_>, SdpError> {
    let mut session_ufrag = None;
    let mut session_pwd = None;
    let mut session_fingerprint = None;
    let mut session_setup = None;
    let mut session_direction = None;

    let mut found_audio = false;
    let mut seen_media = false;
    let mut in_audio = false;
    let mut audio_mid = None;
    let mut audio_ufrag = None;
    let mut audio_pwd = None;
    let mut audio_fingerprint = None;
    let mut audio_setup = None;
    let mut audio_direction = None;
    let mut audio_rtcp_mux = false;
    let mut audio_formats = None;
    let mut audio_opus_payload_type = None;

    for raw_line in sdp.lines() {
        let line = raw_line.trim_end_matches('\r');
        if let Some(media) = line.strip_prefix("m=") {
            seen_media = true;
            in_audio = false;
            let mut fields = media.split_ascii_whitespace();
            let media_type = fields.next().unwrap_or_default();
            if media_type == "audio" && !found_audio {
                let port = fields
                    .next()
                    .and_then(parse_media_port)
                    .ok_or(SdpError::MissingAudioSection)?;
                // A rejected audio m-line cannot carry an active audio stream.
                if port != 0 {
                    found_audio = true;
                    in_audio = true;
                    audio_formats = Some(media);
                }
            }
            continue;
        }

        let Some(value) = line.strip_prefix("a=") else {
            continue;
        };
        if let Some(attribute) = value.strip_prefix("ice-ufrag:") {
            let attribute = attribute.trim();
            if in_audio {
                audio_ufrag = Some(attribute);
            } else if !seen_media {
                session_ufrag = Some(attribute);
            }
        } else if let Some(attribute) = value.strip_prefix("ice-pwd:") {
            let attribute = attribute.trim();
            if in_audio {
                audio_pwd = Some(attribute);
            } else if !seen_media {
                session_pwd = Some(attribute);
            }
        } else if let Some(attribute) = value.strip_prefix("fingerprint:") {
            if in_audio {
                audio_fingerprint =
                    Some(parse_fingerprint(attribute).ok_or(SdpError::MissingFingerprint)?);
            } else if !seen_media {
                session_fingerprint =
                    Some(parse_fingerprint(attribute).ok_or(SdpError::MissingFingerprint)?);
            }
        } else if let Some(attribute) = value.strip_prefix("setup:") {
            if in_audio {
                audio_setup =
                    Some(parse_setup_role(attribute.trim()).ok_or(SdpError::InvalidAttribute)?);
            } else if !seen_media {
                session_setup =
                    Some(parse_setup_role(attribute.trim()).ok_or(SdpError::InvalidAttribute)?);
            }
        } else if let Some(attribute) = value.strip_prefix("mid:") {
            if in_audio {
                audio_mid = Some(attribute.trim());
            }
        } else if value == "rtcp-mux" {
            if in_audio {
                audio_rtcp_mux = true;
            }
        } else if let Some(attribute) = value.strip_prefix("rtpmap:") {
            if in_audio {
                if let Some(payload_type) = parse_opus_mapping(attribute, audio_formats) {
                    audio_opus_payload_type = Some(payload_type);
                }
            }
        } else if let Some(direction) = parse_direction(value) {
            if in_audio {
                audio_direction = Some(direction);
            } else if !seen_media {
                session_direction = Some(direction);
            }
        }
    }

    if !found_audio {
        return Err(SdpError::MissingAudioSection);
    }
    if !audio_rtcp_mux {
        return Err(SdpError::MissingRtcpMux);
    }

    let ice_ufrag = audio_ufrag
        .or(session_ufrag)
        .ok_or(SdpError::MissingIceCredentials)?;
    let ice_pwd = audio_pwd
        .or(session_pwd)
        .ok_or(SdpError::MissingIceCredentials)?;
    if !valid_ice_ufrag(ice_ufrag) || !valid_ice_pwd(ice_pwd) {
        return Err(SdpError::MissingIceCredentials);
    }
    let mid = audio_mid
        .filter(|mid| valid_token(mid))
        .ok_or(SdpError::MissingMid)?;

    Ok(ParsedSdpOffer {
        mid,
        ice_ufrag,
        ice_pwd,
        sha256_fingerprint: audio_fingerprint
            .or(session_fingerprint)
            .ok_or(SdpError::MissingFingerprint)?,
        setup: audio_setup
            .or(session_setup)
            .ok_or(SdpError::MissingDtlsSetup)?,
        opus_payload_type: audio_opus_payload_type.ok_or(SdpError::MissingOpusCodec)?,
        direction: audio_direction
            .or(session_direction)
            .unwrap_or(SdpMediaDirection::SendRecv),
    })
}

pub fn write_sdp_answer(
    offer: &ParsedSdpOffer<'_>,
    local: &LocalSdpParams<'_>,
    out: &mut [u8],
) -> Result<usize, SdpError> {
    let answer_setup = match offer.setup {
        DtlsSetupRole::Active | DtlsSetupRole::ActPass => "passive",
        DtlsSetupRole::Passive => "active",
    };
    let answer_direction = match offer.direction {
        SdpMediaDirection::SendRecv => "sendrecv",
        SdpMediaDirection::SendOnly => "recvonly",
        SdpMediaDirection::RecvOnly => "sendonly",
        SdpMediaDirection::Inactive => "inactive",
    };
    write_sdp_document(
        offer.mid,
        offer.opus_payload_type,
        answer_setup,
        answer_direction,
        local,
        out,
    )
}

pub fn write_sdp_offer(local: &LocalSdpParams<'_>, out: &mut [u8]) -> Result<usize, SdpError> {
    write_sdp_document(
        LOCAL_OFFER_MID,
        LOCAL_OFFER_OPUS_PAYLOAD_TYPE,
        "actpass",
        "sendrecv",
        local,
        out,
    )
}

fn write_sdp_document(
    mid: &str,
    payload_type: u8,
    setup: &str,
    direction: &str,
    local: &LocalSdpParams<'_>,
    out: &mut [u8],
) -> Result<usize, SdpError> {
    let credentials = local.ice_credentials();
    if !valid_token(mid)
        || !valid_ice_ufrag(credentials.ice_ufrag)
        || !valid_ice_pwd(credentials.ice_pwd)
        || local.port == 0
    {
        return Err(SdpError::InvalidAttribute);
    }

    let mut cursor = SliceWriter::new(out);
    let [a, b, c, d] = local.ipv4;
    write!(
        cursor,
        "v=0\r\no=- 1 1 IN IP4 {a}.{b}.{c}.{d}\r\ns=-\r\nt=0 0\r\n\
a=ice-lite\r\na=group:BUNDLE {mid}\r\nm=audio {} UDP/TLS/RTP/SAVPF {payload_type}\r\n\
c=IN IP4 {a}.{b}.{c}.{d}\r\na=mid:{mid}\r\na=ice-ufrag:{}\r\na=ice-pwd:{}\r\n\
a=fingerprint:sha-256 ",
        local.port, credentials.ice_ufrag, credentials.ice_pwd
    )
    .map_err(|_| SdpError::BufferTooSmall)?;

    for (index, byte) in local.sha256_fingerprint.iter().enumerate() {
        if index > 0 {
            write!(cursor, ":").map_err(|_| SdpError::BufferTooSmall)?;
        }
        write!(cursor, "{byte:02X}").map_err(|_| SdpError::BufferTooSmall)?;
    }

    write!(
        cursor,
        "\r\na=setup:{setup}\r\na={direction}\r\na=rtcp-mux\r\n\
a=rtpmap:{payload_type} opus/48000/2\r\n\
a=fmtp:{payload_type} minptime=20;maxplaybackrate=16000;sprop-maxcapturerate=16000;stereo=0;sprop-stereo=0;useinbandfec=1;usedtx=1\r\n\
a=ptime:20\r\na=ssrc:{} cname:intercom-v2\r\na=candidate:1 1 UDP 2130706431 {a}.{b}.{c}.{d} {} typ host\r\n",
        local.ssrc,
        local.port
    )
    .map_err(|_| SdpError::BufferTooSmall)?;

    if local.ipv6 != [0; 8] {
        let [v6_0, v6_1, v6_2, v6_3, v6_4, v6_5, v6_6, v6_7] = local.ipv6;
        write!(
            cursor,
            "a=candidate:2 1 UDP 2130706430 {v6_0:x}:{v6_1:x}:{v6_2:x}:{v6_3:x}:{v6_4:x}:{v6_5:x}:{v6_6:x}:{v6_7:x} {} typ host\r\n",
            local.port
        )
        .map_err(|_| SdpError::BufferTooSmall)?;
    }
    Ok(cursor.written())
}

fn parse_media_port(value: &str) -> Option<u16> {
    value.split('/').next()?.parse().ok()
}

fn parse_opus_mapping(value: &str, formats: Option<&str>) -> Option<u8> {
    let (payload_type, codec) = value.split_once(' ')?;
    let payload_type = payload_type.parse::<u8>().ok()?;
    let declared = formats.is_some_and(|media_line| {
        media_line
            .split_ascii_whitespace()
            .skip(3)
            .any(|format| format.parse::<u8>().ok() == Some(payload_type))
    });
    if declared && codec.eq_ignore_ascii_case("opus/48000/2") {
        Some(payload_type)
    } else {
        None
    }
}

fn parse_fingerprint(value: &str) -> Option<[u8; 32]> {
    let (algorithm, fingerprint) = value.split_once(' ')?;
    if !algorithm.eq_ignore_ascii_case("sha-256") {
        return None;
    }
    parse_colon_hex_32(fingerprint.trim())
}

fn parse_setup_role(value: &str) -> Option<DtlsSetupRole> {
    match value {
        "actpass" => Some(DtlsSetupRole::ActPass),
        "active" => Some(DtlsSetupRole::Active),
        "passive" => Some(DtlsSetupRole::Passive),
        _ => None,
    }
}

fn parse_direction(value: &str) -> Option<SdpMediaDirection> {
    match value {
        "sendrecv" => Some(SdpMediaDirection::SendRecv),
        "sendonly" => Some(SdpMediaDirection::SendOnly),
        "recvonly" => Some(SdpMediaDirection::RecvOnly),
        "inactive" => Some(SdpMediaDirection::Inactive),
        _ => None,
    }
}

fn parse_colon_hex_32(value: &str) -> Option<[u8; 32]> {
    let mut bytes = [0u8; 32];
    let mut count = 0;
    for octet in value.split(':') {
        if count == bytes.len() || octet.len() != 2 {
            return None;
        }
        bytes[count] = u8::from_str_radix(octet, 16).ok()?;
        count += 1;
    }
    (count == bytes.len()).then_some(bytes)
}

fn valid_ice_ufrag(value: &str) -> bool {
    (4..=256).contains(&value.len()) && value.bytes().all(is_ice_char)
}

fn valid_ice_pwd(value: &str) -> bool {
    (22..=256).contains(&value.len()) && value.bytes().all(is_ice_char)
}

fn valid_token(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte))
}

const fn is_ice_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'+' || byte == b'/'
}

struct SliceWriter<'a> {
    buffer: &'a mut [u8],
    position: usize,
}

impl<'a> SliceWriter<'a> {
    fn new(buffer: &'a mut [u8]) -> Self {
        Self {
            buffer,
            position: 0,
        }
    }

    fn written(&self) -> usize {
        self.position
    }
}

impl Write for SliceWriter<'_> {
    fn write_str(&mut self, value: &str) -> core::fmt::Result {
        let end = self
            .position
            .checked_add(value.len())
            .filter(|end| *end <= self.buffer.len())
            .ok_or(core::fmt::Error)?;
        self.buffer[self.position..end].copy_from_slice(value.as_bytes());
        self.position = end;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::{
        parse_sdp_offer, write_sdp_answer, write_sdp_offer, DtlsSetupRole, LocalSdpParams,
        SdpError, SdpMediaDirection,
    };

    const CONTROLLER_OFFER: &str = "v=0\r\n\
o=- 1234 2 IN IP4 127.0.0.1\r\n\
s=-\r\n\
t=0 0\r\n\
a=group:BUNDLE 0\r\n\
m=audio 9 UDP/TLS/RTP/SAVPF 111\r\n\
a=mid:0\r\n\
a=ice-ufrag:ctrlUfrag\r\n\
a=ice-pwd:ctrlPassword0123456789abcdef\r\n\
a=fingerprint:sha-256 AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99\r\n\
a=setup:actpass\r\n\
a=sendrecv\r\n\
a=rtcp-mux\r\n\
a=rtpmap:111 opus/48000/2\r\n";

    #[test]
    fn home_assistant_opus_mapping_is_found_in_audio_payload_list() {
        assert_eq!(
            super::parse_opus_mapping(
                "111 opus/48000/2",
                Some("audio 9 UDP/TLS/RTP/SAVPF 111 63 9 0 8 13 110 126"),
            ),
            Some(111)
        );
    }

    fn sample_local_params() -> LocalSdpParams<'static> {
        LocalSdpParams {
            ipv4: [192, 168, 1, 142],
            ipv6: [0x2001, 0x0db8, 1, 2, 0, 0, 0, 0x0142],
            port: 50_000,
            ice_ufrag: "devUfrag",
            ice_pwd: "devPassword0123456789abcdef",
            sha256_fingerprint: [0x11; 32],
            ssrc: 0x1234_5678,
        }
    }

    #[test]
    fn parses_controller_opus_offer_and_emits_rfc7587_16khz_answer_under_900_bytes() {
        // Arrange
        let mut answer_buffer = [0u8; 2048];

        // Act
        let parsed = parse_sdp_offer(CONTROLLER_OFFER).expect("valid controller offer");
        let length = write_sdp_answer(&parsed, &sample_local_params(), &mut answer_buffer)
            .expect("answer fits");
        let answer = core::str::from_utf8(&answer_buffer[..length]).unwrap();

        // Assert
        assert_eq!(parsed.mid, "0");
        assert_eq!(parsed.ice_ufrag, "ctrlUfrag");
        assert_eq!(parsed.opus_payload_type, 111);
        assert_eq!(parsed.setup, DtlsSetupRole::ActPass);
        assert!(length < 900);
        assert!(answer.contains("a=ice-lite\r\n"));
        assert!(answer.contains("a=rtpmap:111 opus/48000/2\r\n"));
        assert!(answer.contains(
            "maxplaybackrate=16000;sprop-maxcapturerate=16000;stereo=0;sprop-stereo=0;useinbandfec=1"
        ));
    }

    #[test]
    fn inherits_session_credentials_without_leaking_attributes_from_other_media() {
        // Arrange
        let offer = concat!(
            "v=0\r\n",
            "o=- 1234 2 IN IP4 127.0.0.1\r\n",
            "s=-\r\n",
            "t=0 0\r\n",
            "a=ice-ufrag:sessionUfrag\r\n",
            "a=ice-pwd:sessionPassword0123456789abcdef\r\n",
            "a=fingerprint:sha-256 AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99\r\n",
            "a=setup:actpass\r\n",
            "m=video 9 UDP/TLS/RTP/SAVPF 96\r\n",
            "a=ice-ufrag:videoOnly\r\n",
            "a=fingerprint:malformed-video-value\r\n",
            "a=setup:unsupported\r\n",
            "a=rtpmap:96 opus/48000/2\r\n",
            "m=audio 9 UDP/TLS/RTP/SAVPF 111\r\n",
            "a=mid:audio0\r\n",
            "a=rtcp-mux\r\n",
            "a=rtpmap:111 opus/48000/2\r\n",
        );

        // Act
        let parsed = parse_sdp_offer(offer).expect("audio inherits session attributes");

        // Assert
        assert_eq!(parsed.mid, "audio0");
        assert_eq!(parsed.ice_ufrag, "sessionUfrag");
        assert_eq!(parsed.opus_payload_type, 111);
    }

    #[test]
    fn rejects_missing_ice_credentials_and_opus_mapping() {
        // Arrange
        let missing_credentials = "v=0\r\n\
m=audio 9 UDP/TLS/RTP/SAVPF 111\r\n\
a=rtcp-mux\r\n\
a=fingerprint:sha-256 AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99\r\n\
a=setup:actpass\r\n\
a=rtpmap:111 opus/48000/2\r\n";
        let missing_opus = "v=0\r\n\
m=audio 9 UDP/TLS/RTP/SAVPF 111\r\n\
a=mid:0\r\n\
a=rtcp-mux\r\n\
a=ice-ufrag:user\r\n\
a=ice-pwd:password0123456789abcdef\r\n\
a=fingerprint:sha-256 AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99\r\n\
a=setup:actpass\r\n";

        // Act
        let credentials_result = parse_sdp_offer(missing_credentials);
        let opus_result = parse_sdp_offer(missing_opus);

        // Assert
        assert_eq!(credentials_result, Err(SdpError::MissingIceCredentials));
        assert_eq!(opus_result, Err(SdpError::MissingOpusCodec));
    }

    #[test]
    fn rejects_malformed_fingerprint_and_rejected_audio_section() {
        // Arrange
        let malformed_fingerprint = CONTROLLER_OFFER.replace(
            "AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99",
            "AA:BB:CC",
        );
        let rejected_audio = CONTROLLER_OFFER.replace("m=audio 9 ", "m=audio 0 ");

        // Act
        let fingerprint_result = parse_sdp_offer(&malformed_fingerprint);
        let rejected_result = parse_sdp_offer(&rejected_audio);

        // Assert
        assert_eq!(fingerprint_result, Err(SdpError::MissingFingerprint));
        assert_eq!(rejected_result, Err(SdpError::MissingAudioSection));
    }

    #[test]
    fn skips_rejected_audio_section_and_selects_first_active_audio_section() {
        // Arrange
        let offer = CONTROLLER_OFFER.replace(
            "m=audio 9 UDP/TLS/RTP/SAVPF 111\r\n",
            "m=audio 0 UDP/TLS/RTP/SAVPF 111\r\n\
a=rtpmap:111 opus/48000/2\r\n\
m=audio 9 UDP/TLS/RTP/SAVPF 111\r\n",
        );

        // Act
        let parsed = parse_sdp_offer(&offer);

        // Assert
        assert_eq!(parsed.unwrap().mid, "0");
    }

    #[test]
    fn rejects_missing_or_invalid_audio_mid() {
        // Arrange
        let missing_mid = CONTROLLER_OFFER.replace("a=mid:0\r\n", "");
        let invalid_mid = CONTROLLER_OFFER.replace("a=mid:0\r\n", "a=mid:audio track\r\n");

        // Act
        let missing_result = parse_sdp_offer(&missing_mid);
        let invalid_result = parse_sdp_offer(&invalid_mid);

        // Assert
        assert_eq!(missing_result, Err(SdpError::MissingMid));
        assert_eq!(invalid_result, Err(SdpError::MissingMid));
    }

    #[test]
    fn accepts_selected_audio_when_another_media_section_follows() {
        // Arrange
        let offer = std::format!(
            "{CONTROLLER_OFFER}m=video 9 UDP/TLS/RTP/SAVPF 96\r\na=rtpmap:96 VP8/90000\r\n"
        );

        // Act
        let parsed = parse_sdp_offer(&offer);

        // Assert
        assert_eq!(parsed.unwrap().mid, "0");
    }

    #[test]
    fn writes_valid_local_offer_and_reports_small_output_buffer() {
        // Arrange
        let mut offer_buffer = [0u8; 2048];
        let mut short_buffer = [0u8; 8];

        // Act
        let length = write_sdp_offer(&sample_local_params(), &mut offer_buffer).unwrap();
        let offer = core::str::from_utf8(&offer_buffer[..length]).unwrap();
        let short_result = write_sdp_offer(&sample_local_params(), &mut short_buffer);

        // Assert
        assert!(length < 900);
        assert!(offer.contains("a=setup:actpass\r\n"));
        assert!(offer.contains("a=candidate:1 1 UDP 2130706431 192.168.1.142 50000 typ host\r\n"));
        assert_eq!(short_result, Err(SdpError::BufferTooSmall));
    }

    #[test]
    fn writes_ipv4_and_ipv6_host_candidates() {
        // Arrange
        let mut offer_buffer = [0u8; 2048];

        // Act
        let length = write_sdp_offer(&sample_local_params(), &mut offer_buffer).unwrap();
        let offer = core::str::from_utf8(&offer_buffer[..length]).unwrap();

        // Assert
        assert!(offer.contains("a=candidate:1 1 UDP 2130706431 192.168.1.142 50000 typ host\r\n"));
        assert!(offer
            .contains("a=candidate:2 1 UDP 2130706430 2001:db8:1:2:0:0:0:142 50000 typ host\r\n"));
    }

    #[test]
    fn omits_unspecified_ipv6_host_candidate() {
        // Arrange
        let mut params = sample_local_params();
        params.ipv6 = [0; 8];
        let mut offer_buffer = [0u8; 2048];

        // Act
        let length = write_sdp_offer(&params, &mut offer_buffer).unwrap();
        let offer = core::str::from_utf8(&offer_buffer[..length]).unwrap();

        // Assert
        assert!(!offer.contains("a=candidate:2 "));
    }

    #[test]
    fn reverses_send_only_offer_direction_in_answer() {
        // Arrange
        let offer = CONTROLLER_OFFER.replace("a=sendrecv\r\n", "a=sendonly\r\n");
        let mut answer_buffer = [0u8; 2048];

        // Act
        let parsed = parse_sdp_offer(&offer).unwrap();
        let length = write_sdp_answer(&parsed, &sample_local_params(), &mut answer_buffer).unwrap();
        let answer = core::str::from_utf8(&answer_buffer[..length]).unwrap();

        // Assert
        assert!(answer.contains("a=recvonly\r\n"));
    }

    #[test]
    fn inherits_session_direction_and_reverses_it_in_answer() {
        // Arrange
        let offer = CONTROLLER_OFFER.replace("a=sendrecv\r\n", "");
        let offer = offer.replace("t=0 0\r\n", "t=0 0\r\na=sendonly\r\n");
        let mut answer_buffer = [0u8; 2048];

        // Act
        let parsed = parse_sdp_offer(&offer).unwrap();
        let length = write_sdp_answer(&parsed, &sample_local_params(), &mut answer_buffer).unwrap();
        let answer = core::str::from_utf8(&answer_buffer[..length]).unwrap();

        // Assert
        assert_eq!(parsed.direction, SdpMediaDirection::SendOnly);
        assert!(answer.contains("a=recvonly\r\n"));
    }

    #[test]
    fn media_direction_overrides_session_direction() {
        // Arrange
        let offer = CONTROLLER_OFFER.replace("t=0 0\r\n", "t=0 0\r\na=sendonly\r\n");
        let mut answer_buffer = [0u8; 2048];

        // Act
        let parsed = parse_sdp_offer(&offer).unwrap();
        let length = write_sdp_answer(&parsed, &sample_local_params(), &mut answer_buffer).unwrap();
        let answer = core::str::from_utf8(&answer_buffer[..length]).unwrap();

        // Assert
        assert_eq!(parsed.direction, SdpMediaDirection::SendRecv);
        assert!(answer.contains("a=sendrecv\r\n"));
    }
}
