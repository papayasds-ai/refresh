//! Refresh host agent — runs on the Windows PC you want to reach
//! remotely. Listens for connections from already-paired devices only
//! (see refresh-core::pairing) and, once connected, streams the screen
//! and applies incoming input events.
//!
//! This binary deliberately does nothing silently: it logs every
//! connection attempt (accepted or rejected) to stdout/the log file, and
//! it is installed and run the same way any normal Windows application
//! is — there is no hidden-process or silent-autostart behavior in this
//! codebase. If you want it to start with Windows, that's a normal
//! Task Scheduler / Startup folder entry you set up yourself, the same
//! as for any other legitimate application.

mod capture;
mod input;

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use refresh_core::pairing::TrustStore;
use refresh_core::protocol::{Capabilities, Hello, InputEvent, VideoChunk, PROTOCOL_VERSION};
use refresh_core::{io, transport, Identity, DEFAULT_PORT};
use tokio::sync::Mutex as AsyncMutex;

fn data_dir() -> std::path::PathBuf {
    directories::ProjectDirs::from("dev", "refresh", "refresh")
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("./refresh-data"))
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let dir = data_dir();
    let identity = Identity::load_or_create(&dir)?;
    let trust_path = dir.join("trusted_devices.json");
    let trust_store = Arc::new(std::sync::Mutex::new(
        TrustStore::load_or_create(&trust_path).context("loading trust store")?,
    ));

    tracing::info!("device fingerprint: {}", identity.fingerprint());
    tracing::info!(
        "trusted devices: {}",
        trust_store.lock().unwrap().list().len()
    );

    let bind_addr = format!("0.0.0.0:{DEFAULT_PORT}").parse()?;
    let endpoint = transport::server_endpoint(bind_addr, &identity.signing_key, trust_store)
        .context("starting host listener")?;

    tracing::info!("Refresh host listening on UDP {DEFAULT_PORT}");
    tracing::info!("Reachable from a paired device over any network that reaches this machine's IP (e.g. its Tailscale address).");

    while let Some(incoming) = endpoint.accept().await {
        tokio::spawn(async move {
            match incoming.await {
                Ok(conn) => {
                    let peer = conn.remote_address();
                    tracing::info!("connection accepted from {peer}");
                    if let Err(e) = handle_connection(conn).await {
                        tracing::warn!("session with {peer} ended: {e}");
                    }
                }
                Err(e) => tracing::warn!("connection attempt rejected: {e}"),
            }
        });
    }

    Ok(())
}

async fn handle_connection(conn: quinn::Connection) -> Result<()> {
    // Control stream: exchange Hello first so both sides know what this
    // session is allowed to do before any screen data or input moves.
    let (mut ctrl_tx, mut ctrl_rx) = conn.accept_bi().await.context("waiting for control stream")?;
    let _their_hello: Hello = io::read_framed(&mut ctrl_rx).await.context("reading peer Hello")?;
    let our_hello = Hello {
        protocol_version: PROTOCOL_VERSION,
        device_label: "Refresh Windows Host".into(),
        capabilities: Capabilities {
            screen_share: true,
            remote_control: true,
            file_transfer: true,
            clipboard_sync: true,
        },
    };
    io::write_framed(&mut ctrl_tx, &our_hello).await?;

    let conn = Arc::new(conn);

    let video_task = {
        let conn = conn.clone();
        tokio::spawn(async move { video_stream_loop(conn).await })
    };
    let input_task = {
        let conn = conn.clone();
        tokio::spawn(async move { input_stream_loop(conn).await })
    };

    let _ = tokio::join!(video_task, input_task);
    Ok(())
}

/// Captures the screen on a fixed interval and streams JPEG frames to
/// the controller. The interval and quality here favor correctness and
/// simplicity over latency (see capture.rs's note about swapping in a
/// real video codec later).
async fn video_stream_loop(conn: Arc<quinn::Connection>) -> Result<()> {
    let mut send = conn.open_uni().await.context("opening video stream")?;
    let mut sequence: u64 = 0;
    let frame_interval = Duration::from_millis(100); // ~10 fps MVP

    loop {
        let frame = tokio::task::spawn_blocking(capture::capture_frame).await??;
        let jpeg = tokio::task::spawn_blocking({
            let frame = frame;
            move || capture::encode_jpeg(&frame, 60)
        })
        .await??;

        let chunk = VideoChunk {
            sequence,
            is_keyframe: true, // every JPEG frame is independently decodable
            width: 0,          // encoded JPEG carries its own dimensions
            height: 0,
            data: jpeg,
        };
        sequence += 1;

        io::write_framed(&mut send, &chunk).await?;
        tokio::time::sleep(frame_interval).await;
    }
}

/// Reads input events from the controller and applies them locally.
/// Each event is applied as soon as it's decoded — there is no queue
/// that could let input back up and then land unexpectedly later.
async fn input_stream_loop(conn: Arc<quinn::Connection>) -> Result<()> {
    let mut recv = conn.accept_uni().await.context("accepting input stream")?;
    let events_applied = Arc::new(AsyncMutex::new(0u64));
    loop {
        let event: InputEvent = io::read_framed(&mut recv).await?;
        input::apply(&event);
        *events_applied.lock().await += 1;
    }
}
