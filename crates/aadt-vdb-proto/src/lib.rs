//! # aadt-vdb-proto — VDBP, the ShilpiDB wire protocol
//!
//! Length-framed binary request/response codec for talking to a `shilpid` server
//! ([standalone-server plan](../../../docs/plans/shilpidb-standalone-server.md),
//! Phase A). It reuses the exact little-endian primitives ShilpiDB writes on disk
//! ([`aadt_vdb::Writer`] / [`aadt_vdb::Reader`]) so the file format and the wire
//! format share one encoding — no `serde`, no external crate, `unsafe`-free.
//!
//! ## Frame layout
//! ```text
//! u32 body_len | u8 opcode | u32 request_id | payload…
//! ```
//! `body_len` counts `opcode + request_id + payload` (not itself). `request_id`
//! is echoed by the server so a client can correlate responses.

use std::io::{self, Read, Write};

use aadt_geometry::{Bbox, Point2};
use aadt_model::EntityId;
use aadt_vdb::{PointRead, PointWrite, Reader, VdbError, Writer};

/// Wire-protocol version, negotiated in the `HELLO` handshake.
pub const PROTO_VERSION: u32 = 1;

/// Reject a length prefix larger than this (corrupt / hostile). One request is
/// small; large query results stream as `ResultChunk` frames instead.
pub const MAX_FRAME: u32 = 16 * 1024 * 1024;

/// Message kinds. Requests are client→server; responses are server→client.
/// Discriminants are stable across versions — append, never renumber.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum OpCode {
    // Handshake / admin
    Hello = 1,
    HelloOk = 2,
    Ping = 3,
    Pong = 4,
    Use = 5,
    // Reads
    QueryBbox = 10,
    QueryPoint = 11,
    GetEntity = 12,
    Objects = 13,
    Relations = 14,
    // Writes (Phase B) — reserved so opcodes stay stable
    Insert = 20,
    Delete = 21,
    Begin = 22,
    Commit = 23,
    Abort = 24,
    // Responses
    EntityIds = 40,
    ResultChunk = 41,
    ResultEnd = 42,
    // Error
    Err = 255,
}

impl OpCode {
    /// Parse an opcode byte, or `None` if unknown.
    pub fn from_u8(v: u8) -> Option<OpCode> {
        use OpCode::*;
        Some(match v {
            1 => Hello,
            2 => HelloOk,
            3 => Ping,
            4 => Pong,
            5 => Use,
            10 => QueryBbox,
            11 => QueryPoint,
            12 => GetEntity,
            13 => Objects,
            14 => Relations,
            20 => Insert,
            21 => Delete,
            22 => Begin,
            23 => Commit,
            24 => Abort,
            40 => EntityIds,
            41 => ResultChunk,
            42 => ResultEnd,
            255 => Err,
            _ => return None,
        })
    }
}

/// Framing / decoding errors.
#[derive(Debug)]
pub enum ProtoError {
    /// Stream I/O failed (includes a clean EOF as `UnexpectedEof`).
    Io(io::Error),
    /// A payload failed to decode with the binary codec.
    Codec(VdbError),
    /// The opcode byte is not a known [`OpCode`].
    UnknownOpcode(u8),
    /// The frame body is shorter than the fixed header (opcode + request_id).
    ShortFrame,
    /// The length prefix exceeds [`MAX_FRAME`].
    FrameTooLarge(u32),
}

impl std::fmt::Display for ProtoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProtoError::Io(e) => write!(f, "io: {e}"),
            ProtoError::Codec(e) => write!(f, "codec: {e:?}"),
            ProtoError::UnknownOpcode(b) => write!(f, "unknown opcode {b}"),
            ProtoError::ShortFrame => write!(f, "frame shorter than its header"),
            ProtoError::FrameTooLarge(n) => write!(f, "frame length {n} exceeds MAX_FRAME"),
        }
    }
}
impl std::error::Error for ProtoError {}
impl From<io::Error> for ProtoError {
    fn from(e: io::Error) -> Self {
        ProtoError::Io(e)
    }
}
impl From<VdbError> for ProtoError {
    fn from(e: VdbError) -> Self {
        ProtoError::Codec(e)
    }
}

/// One framed message: opcode + client-correlation id + opaque payload bytes.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    pub opcode: OpCode,
    pub request_id: u32,
    pub payload: Vec<u8>,
}

impl Frame {
    pub fn new(opcode: OpCode, request_id: u32, payload: Vec<u8>) -> Self {
        Self {
            opcode,
            request_id,
            payload,
        }
    }
}

/// Write one frame to any stream — a `TcpStream` in the server/client, or a
/// `Vec<u8>` in tests.
pub fn write_frame<W: Write>(w: &mut W, frame: &Frame) -> Result<(), ProtoError> {
    let mut body = Writer::new();
    body.u8(frame.opcode as u8);
    body.u32(frame.request_id);
    body.raw(&frame.payload);
    let body = body.into_bytes();

    let mut hdr = Writer::new();
    hdr.u32(body.len() as u32);
    w.write_all(&hdr.into_bytes())?;
    w.write_all(&body)?;
    Ok(())
}

/// Read one frame from any stream. Blocks until a full frame is available;
/// returns an `Io(UnexpectedEof)` error at a clean end of stream.
pub fn read_frame<R: Read>(r: &mut R) -> Result<Frame, ProtoError> {
    let mut len_buf = [0u8; 4];
    r.read_exact(&mut len_buf)?;
    let len = Reader::new(&len_buf).u32()?; // same LE codec as the writer
    if len < 5 {
        return Err(ProtoError::ShortFrame);
    }
    if len > MAX_FRAME {
        return Err(ProtoError::FrameTooLarge(len));
    }
    let mut body = vec![0u8; len as usize];
    r.read_exact(&mut body)?;

    let mut rd = Reader::new(&body);
    let op = rd.u8()?;
    let opcode = OpCode::from_u8(op).ok_or(ProtoError::UnknownOpcode(op))?;
    let request_id = rd.u32()?;
    let payload = rd.raw((len - 5) as usize)?.to_vec();
    Ok(Frame {
        opcode,
        request_id,
        payload,
    })
}

// --- Typed payload bodies ---------------------------------------------------
//
// Each helper encodes/decodes one message's payload with the shared codec. The
// frame carries them as opaque `payload` bytes.

/// `HELLO` / `HELLO_OK` body: a protocol version + a name (client or server).
pub fn encode_hello(proto_version: u32, name: &str) -> Vec<u8> {
    let mut w = Writer::new();
    w.u32(proto_version);
    w.str(name);
    w.into_bytes()
}
pub fn decode_hello(payload: &[u8]) -> Result<(u32, String), ProtoError> {
    let mut r = Reader::new(payload);
    Ok((r.u32()?, r.str()?))
}

/// `QUERY_BBOX` body: the query window (min, max corners).
pub fn encode_bbox(b: Bbox) -> Vec<u8> {
    let mut w = Writer::new();
    w.point(b.min);
    w.point(b.max);
    w.into_bytes()
}
pub fn decode_bbox(payload: &[u8]) -> Result<Bbox, ProtoError> {
    let mut r = Reader::new(payload);
    let min = r.point()?;
    let max = r.point()?;
    Ok(Bbox { min, max })
}

/// `QUERY_POINT` body: a point + a tolerance.
pub fn encode_point_query(p: Point2, tol: f64) -> Vec<u8> {
    let mut w = Writer::new();
    w.point(p);
    w.f64(tol);
    w.into_bytes()
}
pub fn decode_point_query(payload: &[u8]) -> Result<(Point2, f64), ProtoError> {
    let mut r = Reader::new(payload);
    Ok((r.point()?, r.f64()?))
}

/// `ENTITY_IDS` response body: a length-prefixed list of entity ids.
pub fn encode_entity_ids(ids: &[EntityId]) -> Vec<u8> {
    let mut w = Writer::new();
    w.u32(ids.len() as u32);
    for id in ids {
        w.u64(id.0);
    }
    w.into_bytes()
}
pub fn decode_entity_ids(payload: &[u8]) -> Result<Vec<EntityId>, ProtoError> {
    let mut r = Reader::new(payload);
    let n = r.u32()? as usize;
    let mut ids = Vec::with_capacity(n);
    for _ in 0..n {
        ids.push(EntityId(r.u64()?));
    }
    Ok(ids)
}

/// `ERR` response body: a numeric code + a human message.
pub fn encode_err(code: u32, message: &str) -> Vec<u8> {
    let mut w = Writer::new();
    w.u32(code);
    w.str(message);
    w.into_bytes()
}
pub fn decode_err(payload: &[u8]) -> Result<(u32, String), ProtoError> {
    let mut r = Reader::new(payload);
    Ok((r.u32()?, r.str()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn frame_round_trips_through_a_stream() {
        let frame = Frame::new(OpCode::QueryBbox, 42, vec![1, 2, 3, 4, 5]);
        let mut buf = Vec::new();
        write_frame(&mut buf, &frame).unwrap();
        let got = read_frame(&mut Cursor::new(buf)).unwrap();
        assert_eq!(got, frame);
    }

    #[test]
    fn two_frames_stream_back_to_back() {
        let a = Frame::new(OpCode::Ping, 1, vec![]);
        let b = Frame::new(OpCode::Pong, 2, vec![9, 9]);
        let mut buf = Vec::new();
        write_frame(&mut buf, &a).unwrap();
        write_frame(&mut buf, &b).unwrap();
        let mut cur = Cursor::new(buf);
        assert_eq!(read_frame(&mut cur).unwrap(), a);
        assert_eq!(read_frame(&mut cur).unwrap(), b);
    }

    #[test]
    fn hello_payload_round_trips() {
        let body = encode_hello(PROTO_VERSION, "shilpi-shell");
        let frame = Frame::new(OpCode::Hello, 0, body);
        let mut buf = Vec::new();
        write_frame(&mut buf, &frame).unwrap();
        let got = read_frame(&mut Cursor::new(buf)).unwrap();
        assert_eq!(got.opcode, OpCode::Hello);
        assert_eq!(decode_hello(&got.payload).unwrap(), (PROTO_VERSION, "shilpi-shell".into()));
    }

    #[test]
    fn query_bbox_payload_round_trips() {
        let bbox = Bbox {
            min: Point2::new(-1.0, -2.0),
            max: Point2::new(3.0, 4.5),
        };
        let req = Frame::new(OpCode::QueryBbox, 7, encode_bbox(bbox));
        let mut buf = Vec::new();
        write_frame(&mut buf, &req).unwrap();
        let got = read_frame(&mut Cursor::new(buf)).unwrap();
        assert_eq!(got.request_id, 7);
        let decoded = decode_bbox(&got.payload).unwrap();
        assert_eq!(decoded.min, bbox.min);
        assert_eq!(decoded.max, bbox.max);
    }

    #[test]
    fn entity_ids_response_round_trips() {
        let ids = vec![EntityId(1), EntityId(2), EntityId(999)];
        let resp = Frame::new(OpCode::EntityIds, 7, encode_entity_ids(&ids));
        let mut buf = Vec::new();
        write_frame(&mut buf, &resp).unwrap();
        let got = read_frame(&mut Cursor::new(buf)).unwrap();
        assert_eq!(decode_entity_ids(&got.payload).unwrap(), ids);
    }

    #[test]
    fn unknown_opcode_is_rejected() {
        // Hand-craft a frame whose opcode byte (200) is not a valid OpCode.
        let mut body = Writer::new();
        body.u8(200);
        body.u32(0);
        let body = body.into_bytes();
        let mut w = Writer::new();
        w.u32(body.len() as u32);
        let mut buf = w.into_bytes();
        buf.extend_from_slice(&body);
        match read_frame(&mut Cursor::new(buf)) {
            Err(ProtoError::UnknownOpcode(200)) => {}
            other => panic!("expected UnknownOpcode(200), got {other:?}"),
        }
    }

    #[test]
    fn truncated_stream_reports_eof() {
        // Only a partial length prefix on the wire.
        let buf = vec![5u8, 0]; // 2 of the 4 length bytes
        match read_frame(&mut Cursor::new(buf)) {
            Err(ProtoError::Io(e)) if e.kind() == io::ErrorKind::UnexpectedEof => {}
            other => panic!("expected UnexpectedEof, got {other:?}"),
        }
    }
}
