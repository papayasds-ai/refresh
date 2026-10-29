//! Device trust store and the pairing ceremony.
//!
//! Refresh has no central account system and no server that can see or
//! approve your devices. Trust is purely device-to-device: pairing a new
//! device means its public key gets added to the trust store on the
//! device(s) it should be allowed to reach. Nothing is trusted on first
//! contact — a connecting device that isn't already in the trust store
//! is rejected before any session data is exchanged.
//!
//! The pairing ceremony itself is intentionally simple and auditable:
//! 1. The new device displays its fingerprint (identity::fingerprint).
//! 2. The person reads it (or scans a QR code of it) and enters/confirms
//!    it on the device they're pairing it with.
//! 3. Only after that human confirmation is the public key added to the
//!    trust store.
//! This is the same trust-on-first-use-with-human-verification model SSH
//! and Signal "safety numbers" use, and it is deliberately not something
//! that can be done silently or automatically by software alone.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use ed25519_dalek::VerifyingKey;
use serde::{Deserialize, Serialize};

use crate::identity::fingerprint_of;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustedDevice {
    pub label: String,
    #[serde(with = "key_hex")]
    pub public_key: [u8; 32],
    pub added_unix_secs: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct TrustStore {
    devices: Vec<TrustedDevice>,
}

impl TrustStore {
    pub fn load_or_create(path: &Path) -> Result<Self> {
        if path.exists() {
            let data = fs::read_to_string(path).context("reading trust store")?;
            Ok(serde_json::from_str(&data).context("parsing trust store")?)
        } else {
            Ok(Self::default())
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).ok();
        }
        let data = serde_json::to_string_pretty(self)?;
        fs::write(path, data).context("writing trust store")?;
        Ok(())
    }

    /// Add a device to the trust store. The caller is responsible for
    /// having obtained human confirmation of the fingerprint first —
    /// this function performs no verification of its own, by design,
    /// so that the confirmation step can never accidentally be skipped
    /// by a code path that calls straight into this.
    pub fn add_confirmed(&mut self, label: String, public_key: VerifyingKey) {
        self.devices.retain(|d| d.public_key != public_key.to_bytes());
        self.devices.push(TrustedDevice {
            label,
            public_key: public_key.to_bytes(),
            added_unix_secs: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        });
    }

    pub fn remove(&mut self, public_key: &VerifyingKey) {
        self.devices.retain(|d| d.public_key != public_key.to_bytes());
    }

    pub fn is_trusted(&self, public_key: &VerifyingKey) -> bool {
        self.devices.iter().any(|d| d.public_key == public_key.to_bytes())
    }

    pub fn list(&self) -> &[TrustedDevice] {
        &self.devices
    }
}

/// Render the fingerprint a person should compare during pairing.
pub fn pairing_fingerprint(key: &VerifyingKey) -> String {
    fingerprint_of(key)
}

mod key_hex {
    use serde::{Deserializer, Serializer, Deserialize};

    pub fn serialize<S: Serializer>(key: &[u8; 32], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&hex::encode(key))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 32], D::Error> {
        let s = String::deserialize(d)?;
        let bytes = hex::decode(&s).map_err(serde::de::Error::custom)?;
        bytes
            .try_into()
            .map_err(|_| serde::de::Error::custom("public key must be 32 bytes"))
    }
}
