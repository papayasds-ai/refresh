//! Device identity: a long-term Ed25519 keypair that uniquely and
//! cryptographically identifies this device. The keypair never leaves
//! the device. Trust between two devices is established once, out of
//! band (pairing.rs), by exchanging public keys — after that, every
//! connection is authenticated against the pinned public key, not a
//! password, not a central account, and not a third-party CA.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use ed25519_dalek::{SigningKey, VerifyingKey};
use rand::rngs::OsRng;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

/// A device's persistent identity.
pub struct Identity {
    pub signing_key: SigningKey,
}

impl Identity {
    /// Load this device's identity from disk, generating and persisting
    /// a new one on first run. The private key file is created with
    /// owner-only permissions on unix-like systems.
    pub fn load_or_create(dir: &Path) -> Result<Self> {
        fs::create_dir_all(dir).context("creating identity directory")?;
        let key_path = dir.join("device.key");

        if key_path.exists() {
            let bytes = Zeroizing::new(fs::read(&key_path).context("reading device key")?);
            let arr: [u8; 32] = bytes
                .as_slice()
                .try_into()
                .context("device key file is corrupt (wrong length)")?;
            let signing_key = SigningKey::from_bytes(&arr);
            Ok(Self { signing_key })
        } else {
            let signing_key = SigningKey::generate(&mut OsRng);
            write_private_key(&key_path, &signing_key)?;
            Ok(Self { signing_key })
        }
    }

    pub fn verifying_key(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }

    /// A short, human-comparable fingerprint of this device's public key,
    /// shown during pairing so a person can visually confirm two devices
    /// agree on the same key (defends against a swapped-in-transit key
    /// during pairing).
    pub fn fingerprint(&self) -> String {
        fingerprint_of(&self.verifying_key())
    }
}

pub fn fingerprint_of(key: &VerifyingKey) -> String {
    let digest = Sha256::digest(key.as_bytes());
    // Render as groups of 4 hex chars for easy visual comparison,
    // e.g. "a1b2 c3d4 e5f6 ...".
    let hex = digest.iter().map(|b| format!("{:02x}", b)).collect::<String>();
    hex.as_bytes()
        .chunks(4)
        .map(|c| std::str::from_utf8(c).unwrap())
        .take(8)
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(unix)]
fn write_private_key(path: &PathBuf, key: &SigningKey) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .context("creating device key file")?;
    use std::io::Write;
    f.write_all(&key.to_bytes()).context("writing device key")?;
    Ok(())
}

#[cfg(not(unix))]
fn write_private_key(path: &PathBuf, key: &SigningKey) -> Result<()> {
    fs::write(path, key.to_bytes()).context("writing device key")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_or_create_is_stable_across_calls() {
        let dir = tempfile_dir();
        let id1 = Identity::load_or_create(&dir).unwrap();
        let id2 = Identity::load_or_create(&dir).unwrap();
        assert_eq!(id1.verifying_key(), id2.verifying_key());
        let _ = fs::remove_dir_all(&dir);
    }

    fn tempfile_dir() -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("refresh-test-{}", rand::random::<u64>()));
        p
    }
}
