//! Length-prefixed message framing over any async byte stream (QUIC
//! streams from quinn implement the standard tokio AsyncRead/AsyncWrite
//! traits, so this works unmodified for every stream type in the
//! protocol).

use anyhow::{bail, Result};
use serde::{de::DeserializeOwned, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const MAX_FRAME_BYTES: u32 = 64 * 1024 * 1024; // 64 MiB ceiling against a misbehaving/compromised peer flooding memory

pub async fn write_framed<W: AsyncWriteExt + Unpin, T: Serialize>(w: &mut W, msg: &T) -> Result<()> {
    let body = bincode::serialize(msg)?;
    if body.len() as u64 > MAX_FRAME_BYTES as u64 {
        bail!("outgoing frame too large ({} bytes)", body.len());
    }
    w.write_all(&(body.len() as u32).to_be_bytes()).await?;
    w.write_all(&body).await?;
    Ok(())
}

pub async fn read_framed<R: AsyncReadExt + Unpin, T: DeserializeOwned>(r: &mut R) -> Result<T> {
    let mut len_buf = [0u8; 4];
    r.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf);
    if len > MAX_FRAME_BYTES {
        bail!("incoming frame of {len} bytes exceeds the {MAX_FRAME_BYTES} byte limit - refusing");
    }
    let mut body = vec![0u8; len as usize];
    r.read_exact(&mut body).await?;
    Ok(bincode::deserialize(&body)?)
}
