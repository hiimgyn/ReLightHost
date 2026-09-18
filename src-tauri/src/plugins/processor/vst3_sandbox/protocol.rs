//! Wire protocol between the host and a sandboxed `vst3_sandbox_host` child
//! process, spoken over the child's inherited stdin/stdout pipes.
//!
//! Framing: `[u8 tag][u32 LE payload_len][payload]`.
//!   - `TAG_CONTROL`: JSON-encoded `ControlRequest`/`ControlResponse` — load,
//!     state save/restore, GUI open. Infrequent, not real-time; JSON keeps
//!     this simple instead of hand-rolling a binary schema for it.
//!   - `TAG_PROCESS_REQUEST` / `TAG_PROCESS_RESPONSE`: one audio block, raw
//!     `f32` samples (no JSON/base64) so encode/decode cost stays negligible
//!     against the ~10ms block budget.
//!
//! Only one request is ever in flight per child (the host-side processor is
//! reached through a `Mutex`), so responses need no request-id correlation —
//! whatever comes back next on the pipe is the answer to the last request sent.

use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};

pub const TAG_CONTROL: u8 = 0;
pub const TAG_PROCESS_REQUEST: u8 = 1;
pub const TAG_PROCESS_RESPONSE: u8 = 2;

#[derive(Debug, Serialize, Deserialize)]
pub enum ControlRequest {
    Load { plugin_path: String, sample_rate: f64, block_size: usize },
    GetState,
    SetState { data: Vec<u8> },
    SetParameter { param_id: u32, normalized: f64 },
    /// Ask the child to open the plugin's native editor. The child owns the
    /// whole window (reuses `gui::vst3::win::run_gui_window_impl` unmodified)
    /// and reports the resulting HWND back via `ControlResponse::GuiHwnd` —
    /// HWNDs are session-wide handles, so the host can still `PostMessageW`
    /// WM_CLOSE to it directly without routing that through the pipe.
    OpenGui,
    Shutdown,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ControlResponse {
    Loaded { name: String },
    LoadFailed { error: String },
    State { data: Vec<u8> },
    /// Sent by the child once its GUI thread's window exists.
    GuiHwnd { hwnd: isize },
    /// Sent by the child when its GUI thread's cleanup guard runs (window closed).
    GuiClosed,
    GuiOpenFailed { error: String },
    Ack,
    Error { message: String },
}

pub fn write_frame(w: &mut impl Write, tag: u8, payload: &[u8]) -> io::Result<()> {
    w.write_all(&[tag])?;
    w.write_all(&(payload.len() as u32).to_le_bytes())?;
    w.write_all(payload)?;
    w.flush()
}

/// Reads exactly one frame. Returns `Err(UnexpectedEof)` when the peer's
/// pipe end has closed (child exited / host gave up) — callers treat that
/// as "the other side is gone", not a protocol error.
pub fn read_frame(r: &mut impl Read) -> io::Result<(u8, Vec<u8>)> {
    let mut tag_buf = [0u8; 1];
    r.read_exact(&mut tag_buf)?;
    let mut len_buf = [0u8; 4];
    r.read_exact(&mut len_buf)?;
    let len = u32::from_le_bytes(len_buf) as usize;
    let mut payload = vec![0u8; len];
    r.read_exact(&mut payload)?;
    Ok((tag_buf[0], payload))
}

pub fn write_control(w: &mut impl Write, msg: &ControlRequest) -> io::Result<()> {
    let payload = serde_json::to_vec(msg).map_err(io::Error::other)?;
    write_frame(w, TAG_CONTROL, &payload)
}

pub fn write_control_response(w: &mut impl Write, msg: &ControlResponse) -> io::Result<()> {
    let payload = serde_json::to_vec(msg).map_err(io::Error::other)?;
    write_frame(w, TAG_CONTROL, &payload)
}

pub fn decode_control_request(payload: &[u8]) -> io::Result<ControlRequest> {
    serde_json::from_slice(payload).map_err(io::Error::other)
}

pub fn decode_control_response(payload: &[u8]) -> io::Result<ControlResponse> {
    serde_json::from_slice(payload).map_err(io::Error::other)
}

/// Encode one stereo audio block as `[u32 frame_count][L samples][R samples]`.
pub fn encode_process_block(left: &[f32], right: &[f32]) -> Vec<u8> {
    let n = left.len().min(right.len());
    let mut buf = Vec::with_capacity(4 + n * 8);
    buf.extend_from_slice(&(n as u32).to_le_bytes());
    for &s in &left[..n] { buf.extend_from_slice(&s.to_le_bytes()); }
    for &s in &right[..n] { buf.extend_from_slice(&s.to_le_bytes()); }
    buf
}

/// Decode a block encoded by [`encode_process_block`]. Returns `None` on a
/// truncated/malformed payload rather than panicking — callers treat that
/// the same as a missed block (pass-through).
pub fn decode_process_block(payload: &[u8]) -> Option<(Vec<f32>, Vec<f32>)> {
    if payload.len() < 4 { return None; }
    let n = u32::from_le_bytes(payload[0..4].try_into().ok()?) as usize;
    let need = 4usize.checked_add(n.checked_mul(8)?)?;
    if payload.len() < need { return None; }
    let mut left = Vec::with_capacity(n);
    let mut right = Vec::with_capacity(n);
    let mut off = 4;
    for _ in 0..n {
        left.push(f32::from_le_bytes(payload[off..off + 4].try_into().ok()?));
        off += 4;
    }
    for _ in 0..n {
        right.push(f32::from_le_bytes(payload[off..off + 4].try_into().ok()?));
        off += 4;
    }
    Some((left, right))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn frame_roundtrip() {
        let mut buf = Vec::new();
        write_frame(&mut buf, 7, b"hello").unwrap();
        let (tag, payload) = read_frame(&mut Cursor::new(buf)).unwrap();
        assert_eq!(tag, 7);
        assert_eq!(payload, b"hello");
    }

    #[test]
    fn control_request_roundtrip() {
        let mut buf = Vec::new();
        let req = ControlRequest::Load {
            plugin_path: "C:\\plugins\\Clear.vst3".into(),
            sample_rate: 48000.0,
            block_size: 512,
        };
        write_control(&mut buf, &req).unwrap();
        let (tag, payload) = read_frame(&mut Cursor::new(buf)).unwrap();
        assert_eq!(tag, TAG_CONTROL);
        match decode_control_request(&payload).unwrap() {
            ControlRequest::Load { plugin_path, sample_rate, block_size } => {
                assert_eq!(plugin_path, "C:\\plugins\\Clear.vst3");
                assert_eq!(sample_rate, 48000.0);
                assert_eq!(block_size, 512);
            }
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn process_block_roundtrip() {
        let left = vec![0.1f32, -0.2, 0.3, 1.0, -1.0];
        let right = vec![-0.5f32, 0.25, 0.0, -0.75, 0.125];
        let encoded = encode_process_block(&left, &right);
        let (decoded_left, decoded_right) = decode_process_block(&encoded).unwrap();
        assert_eq!(decoded_left, left);
        assert_eq!(decoded_right, right);
    }

    #[test]
    fn process_block_mismatched_lengths_uses_shorter() {
        let left = vec![1.0f32, 2.0, 3.0];
        let right = vec![9.0f32, 8.0];
        let encoded = encode_process_block(&left, &right);
        let (decoded_left, decoded_right) = decode_process_block(&encoded).unwrap();
        assert_eq!(decoded_left, vec![1.0, 2.0]);
        assert_eq!(decoded_right, vec![9.0, 8.0]);
    }

    #[test]
    fn process_block_truncated_payload_is_none() {
        assert!(decode_process_block(&[5, 0, 0, 0, 1, 2]).is_none());
    }

    #[test]
    fn process_block_empty() {
        let encoded = encode_process_block(&[], &[]);
        let (l, r) = decode_process_block(&encoded).unwrap();
        assert!(l.is_empty() && r.is_empty());
    }
}
