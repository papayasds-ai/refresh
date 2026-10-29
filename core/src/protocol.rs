//! Wire protocol for an established, already-authenticated session.
//! By the time any of these messages are sent, the QUIC/TLS layer in
//! `transport.rs` has already verified both devices' identity keys
//! against the trust store, so this layer only has to worry about
//! *what* to say, not *who it's talking to*.
//!
//! Each logical stream below is its own QUIC stream, so a large file
//! transfer can never block input events, and input events can never
//! block the video stream.

use serde::{Deserialize, Serialize};

/// Sent once, immediately after the QUIC handshake completes, on a
/// dedicated control stream. Lets both sides confirm protocol version
/// and agree on what the session is allowed to do before any screen
/// data, input, or files are exchanged.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hello {
    pub protocol_version: u32,
    pub device_label: String,
    pub capabilities: Capabilities,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct Capabilities {
    pub screen_share: bool,
    pub remote_control: bool,
    pub file_transfer: bool,
    pub clipboard_sync: bool,
}

pub const PROTOCOL_VERSION: u32 = 1;

/// Input events sent from controller -> host, on their own stream so
/// they stay low-latency regardless of what else is happening.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InputEvent {
    MouseMove { x: i32, y: i32 },
    MouseButton { button: MouseButton, pressed: bool },
    MouseScroll { delta_x: i32, delta_y: i32 },
    Key { code: u32, pressed: bool },
    /// Touch events from the Android client are translated to this
    /// on receipt by the host rather than carrying raw touch state,
    /// so the host-side input layer only ever has one input model.
    Touch { x: f32, y: f32, phase: TouchPhase },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum TouchPhase {
    Start,
    Move,
    End,
}

/// A chunk of encoded video on the screen-share stream. Framing is kept
/// intentionally simple (sequence number + a flag for keyframes) so the
/// codec itself can be swapped later without touching the transport.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoChunk {
    pub sequence: u64,
    pub is_keyframe: bool,
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClipboardUpdate {
    Text(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FileTransferMessage {
    Offer { name: String, size_bytes: u64, sha256: [u8; 32] },
    Accept,
    Reject,
    Chunk { offset: u64, data: Vec<u8> },
    Complete,
}

/// Serialize a message for the wire. Centralized here so every stream
/// uses the same (length-prefixed bincode) framing and nothing hand-rolls
/// its own parser.
pub fn encode<T: Serialize>(msg: &T) -> anyhow::Result<Vec<u8>> {
    let body = bincode::serialize(msg)?;
    let mut out = Vec::with_capacity(4 + body.len());
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.extend_from_slice(&body);
    Ok(out)
}

pub fn decode<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> anyhow::Result<T> {
    if bytes.len() < 4 {
        anyhow::bail!("frame too short");
    }
    let len = u32::from_be_bytes(bytes[0..4].try_into().unwrap()) as usize;
    let body = bytes.get(4..4 + len).ok_or_else(|| anyhow::anyhow!("frame length mismatch"))?;
    Ok(bincode::deserialize(body)?)
}
