//! Bounded local transport framing and peer policy (local preparation).
//!
//! Transport selection: local Unix-domain socket under an operator-owned
//! directory (`0700`), JSON payloads, four-byte big-endian length-prefixed
//! frames. No shell-string transport, no terminal scraping, no shared mutable
//! file, and no upstream endpoint: the peer is always the local coordinator
//! or its admitted host client. This module defines framing, limits, peer
//! policy, and typed errors as pure validation; socket input/output attaches
//! in a later stage against exactly these types without changing them.

use core::fmt;

use serde::{Deserialize, Serialize};

/// Transport selection identifier pinned by this contract.
pub const TRANSPORT_KIND: &str = "unix-socket-json-v1";

/// Default maximum frame size in bytes (one mebibyte).
pub const DEFAULT_MAX_FRAME_BYTES: usize = 1_048_576;

/// Minimum admissible maximum frame size in bytes.
pub const MIN_MAX_FRAME_BYTES: usize = 1_024;

/// Maximum admissible maximum frame size in bytes (sixteen mebibytes).
pub const MAX_MAX_FRAME_BYTES: usize = 16_777_216;

/// Default per-connection request ceiling.
pub const DEFAULT_MAX_REQUESTS_PER_CONNECTION: u32 = 128;

/// Default read deadline in milliseconds.
pub const DEFAULT_READ_TIMEOUT_MS: u64 = 30_000;

/// Default write deadline in milliseconds.
pub const DEFAULT_WRITE_TIMEOUT_MS: u64 = 10_000;

/// Minimum admissible deadline in milliseconds.
pub const MIN_TIMEOUT_MS: u64 = 100;

/// Maximum admissible deadline in milliseconds (ten minutes).
pub const MAX_TIMEOUT_MS: u64 = 600_000;

/// Maximum admissible per-connection request ceiling.
pub const MAX_REQUESTS_PER_CONNECTION: u32 = 10_000;

/// Length-prefix width in bytes (unsigned 32-bit big-endian).
pub const FRAME_HEADER_LEN: usize = 4;

/// Bounded transport limits agreed before any byte flows.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TransportLimits {
    /// Maximum decoded payload size in bytes.
    pub max_frame_bytes: usize,
    /// Maximum requests accepted on one connection.
    pub max_requests_per_connection: u32,
    /// Read deadline in milliseconds.
    pub read_timeout_ms: u64,
    /// Write deadline in milliseconds.
    pub write_timeout_ms: u64,
}

/// Stable transport error.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostTransportError {
    /// Limit values are outside their admissible bounds.
    InvalidLimits,
    /// A zero-length payload was offered for framing.
    EmptyFrame,
    /// A payload or advertised length exceeds the bound.
    OversizeFrame,
    /// Fewer bytes are available than the frame requires.
    TruncatedFrame,
    /// Frame bytes are structurally unusable.
    MalformedFrame,
    /// Observed socket-directory ownership or mode is unsafe.
    PeerRefused,
}

impl fmt::Display for HostTransportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidLimits => "codingmage.host.transport.invalid_limits",
            Self::EmptyFrame => "codingmage.host.transport.empty_frame",
            Self::OversizeFrame => "codingmage.host.transport.oversize_frame",
            Self::TruncatedFrame => "codingmage.host.transport.truncated_frame",
            Self::MalformedFrame => "codingmage.host.transport.malformed_frame",
            Self::PeerRefused => "codingmage.host.transport.peer_refused",
        })
    }
}

impl std::error::Error for HostTransportError {}

impl TransportLimits {
    /// Conservative default limits.
    #[must_use]
    pub const fn default_limits() -> Self {
        Self {
            max_frame_bytes: DEFAULT_MAX_FRAME_BYTES,
            max_requests_per_connection: DEFAULT_MAX_REQUESTS_PER_CONNECTION,
            read_timeout_ms: DEFAULT_READ_TIMEOUT_MS,
            write_timeout_ms: DEFAULT_WRITE_TIMEOUT_MS,
        }
    }

    /// Validates every bound.
    ///
    /// # Errors
    ///
    /// Returns [`HostTransportError::InvalidLimits`] when any bound is
    /// outside its admissible range or a deadline is zero.
    pub fn verify(&self) -> Result<(), HostTransportError> {
        if !(MIN_MAX_FRAME_BYTES..=MAX_MAX_FRAME_BYTES).contains(&self.max_frame_bytes)
            || self.max_requests_per_connection == 0
            || self.max_requests_per_connection > MAX_REQUESTS_PER_CONNECTION
            || !(MIN_TIMEOUT_MS..=MAX_TIMEOUT_MS).contains(&self.read_timeout_ms)
            || !(MIN_TIMEOUT_MS..=MAX_TIMEOUT_MS).contains(&self.write_timeout_ms)
        {
            return Err(HostTransportError::InvalidLimits);
        }
        Ok(())
    }
}

/// Encodes one payload as a length-prefixed frame.
///
/// # Errors
///
/// Returns [`HostTransportError::EmptyFrame`] for an empty payload and
/// [`HostTransportError::OversizeFrame`] when the payload exceeds the bound.
pub fn encode_frame(
    payload: &[u8],
    limits: &TransportLimits,
) -> Result<Vec<u8>, HostTransportError> {
    limits.verify()?;
    if payload.is_empty() {
        return Err(HostTransportError::EmptyFrame);
    }
    if payload.len() > limits.max_frame_bytes {
        return Err(HostTransportError::OversizeFrame);
    }
    let Ok(length) = u32::try_from(payload.len()) else {
        return Err(HostTransportError::OversizeFrame);
    };
    let mut frame = Vec::with_capacity(FRAME_HEADER_LEN + payload.len());
    frame.extend_from_slice(&length.to_be_bytes());
    frame.extend_from_slice(payload);
    Ok(frame)
}

/// Decodes the first frame in `buffer`, returning the payload and the bytes
/// consumed. Trailing bytes belong to later frames and are left untouched.
///
/// # Errors
///
/// Returns [`HostTransportError::TruncatedFrame`] when fewer bytes are
/// available than the header or the advertised payload requires,
/// [`HostTransportError::MalformedFrame`] for a zero advertised length, and
/// [`HostTransportError::OversizeFrame`] when the advertised length exceeds
/// the bound.
pub fn decode_frame(
    buffer: &[u8],
    limits: &TransportLimits,
) -> Result<(Vec<u8>, usize), HostTransportError> {
    limits.verify()?;
    let Some(header) = buffer.get(..FRAME_HEADER_LEN) else {
        return Err(HostTransportError::TruncatedFrame);
    };
    let header: [u8; 4] = header
        .try_into()
        .map_err(|_| HostTransportError::MalformedFrame)?;
    let length = u32::from_be_bytes(header);
    if length == 0 {
        return Err(HostTransportError::MalformedFrame);
    }
    let Ok(length) = usize::try_from(length) else {
        return Err(HostTransportError::OversizeFrame);
    };
    if length > limits.max_frame_bytes {
        return Err(HostTransportError::OversizeFrame);
    }
    let end = FRAME_HEADER_LEN.saturating_add(length);
    let Some(payload) = buffer.get(FRAME_HEADER_LEN..end) else {
        return Err(HostTransportError::TruncatedFrame);
    };
    Ok((payload.to_vec(), end))
}

/// Validates observed socket-directory ownership and mode before any peer is
/// trusted.
///
/// The input/output layer supplies the observed owner uid and mode bits; this
/// check admits only directories owned by `expected_uid` with no group or
/// other permission bits. Anything else fails closed.
///
/// # Errors
///
/// Returns [`HostTransportError::PeerRefused`] for foreign ownership or any
/// group/other permission bit.
pub fn validate_peer_directory(
    observed_uid: u32,
    observed_mode: u32,
    expected_uid: u32,
) -> Result<(), HostTransportError> {
    if observed_uid != expected_uid || observed_mode & 0o077 != 0 {
        return Err(HostTransportError::PeerRefused);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> TransportLimits {
        TransportLimits::default_limits()
    }

    #[test]
    fn default_limits_verify() {
        assert!(limits().verify().is_ok());
    }

    #[test]
    fn rejects_out_of_bound_limits() {
        for limits in [
            TransportLimits {
                max_frame_bytes: MIN_MAX_FRAME_BYTES - 1,
                ..limits()
            },
            TransportLimits {
                max_frame_bytes: MAX_MAX_FRAME_BYTES + 1,
                ..limits()
            },
            TransportLimits {
                max_requests_per_connection: 0,
                ..limits()
            },
            TransportLimits {
                max_requests_per_connection: MAX_REQUESTS_PER_CONNECTION + 1,
                ..limits()
            },
            TransportLimits {
                read_timeout_ms: MIN_TIMEOUT_MS - 1,
                ..limits()
            },
            TransportLimits {
                write_timeout_ms: MAX_TIMEOUT_MS + 1,
                ..limits()
            },
        ] {
            assert_eq!(limits.verify(), Err(HostTransportError::InvalidLimits));
        }
    }

    #[test]
    fn frame_round_trip_is_exact_and_leaves_trailing_bytes() {
        let payload = b"{\"protocol_version\":1}";
        let mut stream = encode_frame(payload, &limits()).expect("valid fixture");
        stream.extend_from_slice(&encode_frame(b"[1]", &limits()).expect("valid fixture"));
        let (first, consumed) = decode_frame(&stream, &limits()).expect("valid fixture");
        assert_eq!(first, payload);
        let (second, used) = decode_frame(&stream[consumed..], &limits()).expect("valid fixture");
        assert_eq!(second, b"[1]");
        assert_eq!(consumed + used, stream.len());
    }

    #[test]
    fn accepts_boundary_sized_payload() {
        let bound = TransportLimits {
            max_frame_bytes: MIN_MAX_FRAME_BYTES,
            ..limits()
        };
        let payload = vec![0x41; MIN_MAX_FRAME_BYTES];
        let frame = encode_frame(&payload, &bound).expect("valid fixture");
        assert_eq!(frame.len(), FRAME_HEADER_LEN + MIN_MAX_FRAME_BYTES);
        let (decoded, consumed) = decode_frame(&frame, &bound).expect("valid fixture");
        assert_eq!(decoded, payload);
        assert_eq!(consumed, frame.len());
    }

    #[test]
    fn rejects_empty_and_oversize_payloads() {
        assert_eq!(
            encode_frame(&[], &limits()),
            Err(HostTransportError::EmptyFrame)
        );
        let bound = TransportLimits {
            max_frame_bytes: MIN_MAX_FRAME_BYTES,
            ..limits()
        };
        let oversized = vec![0x41; MIN_MAX_FRAME_BYTES + 1];
        assert_eq!(
            encode_frame(&oversized, &bound),
            Err(HostTransportError::OversizeFrame)
        );
    }

    #[test]
    fn rejects_truncated_and_malformed_frames() {
        assert_eq!(
            decode_frame(&[0x00, 0x00], &limits()),
            Err(HostTransportError::TruncatedFrame)
        );
        assert_eq!(
            decode_frame(&[0x00; 4], &limits()),
            Err(HostTransportError::MalformedFrame)
        );
        let mut short = encode_frame(b"abcdef", &limits()).expect("valid fixture");
        short.truncate(5);
        assert_eq!(
            decode_frame(&short, &limits()),
            Err(HostTransportError::TruncatedFrame)
        );
        let mut hostile = 0x00FF_FFFF_u32.to_be_bytes().to_vec();
        hostile.extend_from_slice(b"!");
        assert_eq!(
            decode_frame(&hostile, &limits()),
            Err(HostTransportError::OversizeFrame)
        );
    }

    #[test]
    fn refuses_foreign_peer_ownership_and_modes() {
        assert!(validate_peer_directory(1000, 0o700, 1000).is_ok());
        for (uid, mode) in [
            (1001, 0o700),
            (1000, 0o750),
            (1000, 0o707),
            (1000, 0o777),
            (0, 0o700),
        ] {
            assert_eq!(
                validate_peer_directory(uid, mode, 1000),
                Err(HostTransportError::PeerRefused),
                "{uid:o}/{mode:o}"
            );
        }
    }

    #[test]
    fn transport_error_codes_are_stable() {
        assert_eq!(
            HostTransportError::InvalidLimits.to_string(),
            "codingmage.host.transport.invalid_limits"
        );
        assert_eq!(
            HostTransportError::EmptyFrame.to_string(),
            "codingmage.host.transport.empty_frame"
        );
        assert_eq!(
            HostTransportError::OversizeFrame.to_string(),
            "codingmage.host.transport.oversize_frame"
        );
        assert_eq!(
            HostTransportError::TruncatedFrame.to_string(),
            "codingmage.host.transport.truncated_frame"
        );
        assert_eq!(
            HostTransportError::MalformedFrame.to_string(),
            "codingmage.host.transport.malformed_frame"
        );
        assert_eq!(
            HostTransportError::PeerRefused.to_string(),
            "codingmage.host.transport.peer_refused"
        );
    }

    #[test]
    fn transport_selection_is_pinned() {
        assert_eq!(TRANSPORT_KIND, "unix-socket-json-v1");
        assert_eq!(FRAME_HEADER_LEN, 4);
    }
}
