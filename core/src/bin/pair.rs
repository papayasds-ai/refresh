//! `pair` — a small CLI used on both devices during the one-time pairing
//! ceremony described in pairing.rs. It never talks over the network:
//! it only prints this device's identity/fingerprint, or records a
//! fingerprint+key you confirm by eye (or by scanning a QR code that
//! encodes the same string) as trusted.
//!
//! Usage:
//!   pair show                     - print this device's pairing code
//!   pair trust <pairing-code> <label>
//!                                  - add a device after confirming its
//!                                    fingerprint out of band

use std::path::PathBuf;

use anyhow::{anyhow, Result};
use ed25519_dalek::VerifyingKey;
use refresh_core::identity::Identity;
use refresh_core::pairing::{pairing_fingerprint, TrustStore};

fn data_dir() -> PathBuf {
    directories::ProjectDirs::from("dev", "refresh", "refresh")
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("./refresh-data"))
}

/// The pairing code is the thing you actually copy between devices
/// (or encode as a QR). It bundles the public key with its own
/// fingerprint so the receiving side can confirm both match before
/// trusting it.
fn encode_pairing_code(key: &VerifyingKey) -> String {
    format!("{}.{}", hex::encode(key.as_bytes()), pairing_fingerprint(key).replace(' ', ""))
}

fn decode_pairing_code(code: &str) -> Result<VerifyingKey> {
    let (key_hex, fp_claim) = code
        .split_once('.')
        .ok_or_else(|| anyhow!("malformed pairing code"))?;
    let key_bytes = hex::decode(key_hex)?;
    let arr: [u8; 32] = key_bytes
        .as_slice()
        .try_into()
        .map_err(|_| anyhow!("pairing code has wrong key length"))?;
    let key = VerifyingKey::from_bytes(&arr)?;
    let actual_fp = pairing_fingerprint(&key).replace(' ', "");
    if actual_fp != fp_claim {
        return Err(anyhow!(
            "pairing code is internally inconsistent (fingerprint mismatch) - do not trust this code"
        ));
    }
    Ok(key)
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let dir = data_dir();
    let identity = Identity::load_or_create(&dir)?;

    match args.get(1).map(String::as_str) {
        Some("show") => {
            let code = encode_pairing_code(&identity.verifying_key());
            println!("This device's pairing code:\n");
            println!("  {code}\n");
            println!("Fingerprint (read this aloud to confirm on the other device):");
            println!("  {}\n", identity.fingerprint());
            println!("Enter this exact code on the OTHER device with:");
            println!("  pair trust {code} \"<name for this device>\"");
        }
        Some("trust") => {
            let code = args
                .get(2)
                .ok_or_else(|| anyhow!("usage: pair trust <pairing-code> <label>"))?;
            let label = args.get(3).cloned().unwrap_or_else(|| "unnamed device".into());

            let key = decode_pairing_code(code)?;
            println!("About to trust device \"{label}\" with fingerprint:");
            println!("  {}", pairing_fingerprint(&key));
            println!("\nConfirm this matches what is shown on the OTHER device before continuing.");
            print!("Type 'yes' to confirm: ");
            use std::io::Write;
            std::io::stdout().flush().ok();
            let mut answer = String::new();
            std::io::stdin().read_line(&mut answer)?;
            if answer.trim() != "yes" {
                println!("Not trusted. Nothing was added.");
                return Ok(());
            }

            let trust_path = dir.join("trusted_devices.json");
            let mut store = TrustStore::load_or_create(&trust_path)?;
            store.add_confirmed(label.clone(), key);
            store.save(&trust_path)?;
            println!("Trusted \"{label}\". It can now connect to this device.");
        }
        Some("list") => {
            let trust_path = dir.join("trusted_devices.json");
            let store = TrustStore::load_or_create(&trust_path)?;
            if store.list().is_empty() {
                println!("No trusted devices yet.");
            }
            for d in store.list() {
                println!("- {} ({})", d.label, hex::encode(d.public_key));
            }
        }
        _ => {
            println!("Usage:");
            println!("  pair show                              Show this device's pairing code");
            println!("  pair trust <code> <label>               Trust another device");
            println!("  pair list                                List trusted devices");
        }
    }
    Ok(())
}
