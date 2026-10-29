//! UniFFI bridge: exposes refresh-core's identity/pairing/transport code
//! to Kotlin. The reason this exists at all, instead of reimplementing
//! the protocol in Kotlin, is deliberate: the cryptography, trust
//! verification, and wire protocol are the parts of this system where a
//! second, independent implementation would be a second place for
//! security bugs to hide. Android gets exactly the same Rust transport
//! code the Windows host uses, compiled for `aarch64-linux-android` /
//! etc. via `cargo ndk`, and only talks to it through this thin,
//! non-cryptographic surface.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use ed25519_dalek::VerifyingKey;
use once_cell::sync::OnceCell;
use refresh_core::pairing::{pairing_fingerprint, TrustStore};
use refresh_core::protocol::{Capabilities, Hello, InputEvent, MouseButton, TouchPhase, VideoChunk, PROTOCOL_VERSION};
use refresh_core::{io as framed, transport, Identity};
use tokio::runtime::Runtime;
use tokio::sync::mpsc;

uniffi::include_scaffolding!("refresh");

#[derive(Debug, thiserror::Error)]
pub enum FfiError {
    #[error("{0}")]
    Failed(String),
}

impl From<anyhow::Error> for FfiError {
    fn from(e: anyhow::Error) -> Self {
        FfiError::Failed(format!("{e:#}"))
    }
}

fn runtime() -> &'static Runtime {
    static RT: OnceCell<Runtime> = OnceCell::new();
    RT.get_or_init(|| Runtime::new().expect("failed to start async runtime"))
}

fn encode_pairing_code(key: &VerifyingKey) -> String {
    format!("{}.{}", hex::encode(key.as_bytes()), pairing_fingerprint(key).replace(' ', ""))
}

fn decode_pairing_code(code: &str) -> anyhow::Result<VerifyingKey> {
    let (key_hex, fp_claim) = code
        .split_once('.')
        .ok_or_else(|| anyhow::anyhow!("malformed pairing code"))?;
    let key_bytes = hex::decode(key_hex)?;
    let arr: [u8; 32] = key_bytes
        .as_slice()
        .try_into()
        .map_err(|_| anyhow::anyhow!("pairing code has wrong key length"))?;
    let key = VerifyingKey::from_bytes(&arr)?;
    if pairing_fingerprint(&key).replace(' ', "") != fp_claim {
        anyhow::bail!("pairing code failed its own integrity check - do not trust it");
    }
    Ok(key)
}

pub struct RefreshClient {
    identity: Identity,
    trust_store: Arc<Mutex<TrustStore>>,
    trust_path: PathBuf,
}

impl RefreshClient {
    pub fn new(data_dir: String) -> Result<Self, FfiError> {
        let dir = PathBuf::from(data_dir);
        let identity = Identity::load_or_create(&dir).map_err(anyhow::Error::from)?;
        let trust_path = dir.join("trusted_devices.json");
        let trust_store = TrustStore::load_or_create(&trust_path).map_err(anyhow::Error::from)?;
        Ok(Self {
            identity,
            trust_store: Arc::new(Mutex::new(trust_store)),
            trust_path,
        })
    }

    pub fn fingerprint(&self) -> String {
        self.identity.fingerprint()
    }

    pub fn pairing_code(&self) -> String {
        encode_pairing_code(&self.identity.verifying_key())
    }

    /// Caller (the Kotlin UI) is responsible for showing the decoded
    /// fingerprint to the person and getting their explicit confirmation
    /// *before* calling this — see pairing.rs's doc comment for why that
    /// ordering matters.
    pub fn trust_device(&self, code: String, label: String) -> Result<(), FfiError> {
        let key = decode_pairing_code(&code).map_err(FfiError::from)?;
        let mut store = self.trust_store.lock().unwrap();
        store.add_confirmed(label, key);
        store.save(&self.trust_path).map_err(FfiError::from)?;
        Ok(())
    }

    pub fn list_trusted(&self) -> Vec<String> {
        self.trust_store
            .lock()
            .unwrap()
            .list()
            .iter()
            .map(|d| d.label.clone())
            .collect()
    }

    pub fn connect(&self, host: String, port: u16) -> Result<Arc<RefreshSession>, FfiError> {
        let signing_key = self.identity.signing_key.clone();
        let trust_store = self.trust_store.clone();

        runtime()
            .block_on(async move { connect_inner(host, port, signing_key, trust_store).await })
            .map_err(FfiError::from)
    }
}

pub struct RefreshSession {
    input_tx: mpsc::UnboundedSender<InputEvent>,
    frame_rx: Mutex<mpsc::UnboundedReceiver<Vec<u8>>>,
    _conn: quinn::Connection,
}

impl RefreshSession {
    pub fn send_mouse_move(&self, x: i32, y: i32) {
        let _ = self.input_tx.send(InputEvent::MouseMove { x, y });
    }

    pub fn send_touch(&self, x: f32, y: f32, phase: String) {
        let phase = match phase.as_str() {
            "start" => TouchPhase::Start,
            "end" => TouchPhase::End,
            _ => TouchPhase::Move,
        };
        let _ = self.input_tx.send(InputEvent::Touch { x, y, phase });
    }

    pub fn next_frame(&self, timeout_ms: u32) -> Result<Vec<u8>, FfiError> {
        runtime().block_on(async {
            let mut rx = self.frame_rx.lock().unwrap();
            match tokio::time::timeout(
                std::time::Duration::from_millis(timeout_ms as u64),
                rx.recv(),
            )
            .await
            {
                Ok(Some(bytes)) => Ok(bytes),
                Ok(None) => Err(FfiError::Failed("session closed".into())),
                Err(_elapsed) => Ok(Vec::new()),
            }
        })
    }

    pub fn close(&self) {
        self._conn.close(0u32.into(), b"closed by client");
    }
}

// Silences an unused-import warning for MouseButton, which is part of
// the shared protocol surface even though this MVP client only emits
// mouse-move and touch events so far.
#[allow(dead_code)]
fn _unused(_: MouseButton) {}

async fn connect_inner(
    host: String,
    port: u16,
    signing_key: ed25519_dalek::SigningKey,
    trust_store: Arc<Mutex<TrustStore>>,
) -> anyhow::Result<Arc<RefreshSession>> {
    let endpoint = transport::client_endpoint(&signing_key, trust_store)?;
    let addr: std::net::SocketAddr = format!("{host}:{port}").parse()?;
    let connecting = endpoint.connect(addr, "refresh.local")?;
    let conn = connecting.await?;

    let (mut ctrl_tx, mut ctrl_rx) = conn.open_bi().await?;
    let our_hello = Hello {
        protocol_version: PROTOCOL_VERSION,
        device_label: "Refresh Android Client".into(),
        capabilities: Capabilities {
            screen_share: true,
            remote_control: true,
            file_transfer: false,
            clipboard_sync: false,
        },
    };
    framed::write_framed(&mut ctrl_tx, &our_hello).await?;
    let _their_hello: Hello = framed::read_framed(&mut ctrl_rx).await?;

    // Video: host opens this uni stream and sends frames; we accept it.
    let mut video_recv = conn.accept_uni().await?;
    let (frame_tx, frame_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    tokio::spawn(async move {
        loop {
            match framed::read_framed::<_, VideoChunk>(&mut video_recv).await {
                Ok(chunk) => {
                    if frame_tx.send(chunk.data).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    // Input: we open this uni stream and send events the host applies.
    let mut input_send = conn.open_uni().await?;
    let (input_tx, mut input_rx) = mpsc::unbounded_channel::<InputEvent>();
    tokio::spawn(async move {
        while let Some(event) = input_rx.recv().await {
            if framed::write_framed(&mut input_send, &event).await.is_err() {
                break;
            }
        }
    });

    Ok(Arc::new(RefreshSession {
        input_tx,
        frame_rx: Mutex::new(frame_rx),
        _conn: conn,
    }))
}
