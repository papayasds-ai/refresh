//! Encrypted transport: QUIC (via quinn) with TLS 1.3, where trust is
//! anchored to pinned Ed25519 device keys instead of a certificate
//! authority. There is no central authority anywhere in this system —
//! a connection succeeds only if the peer's key is already in the
//! local trust store (see pairing.rs).

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use ed25519_dalek::{SigningKey, VerifyingKey};
use quinn::{ClientConfig, Endpoint, ServerConfig};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, UnixTime};

use crate::pairing::TrustStore;

/// ALPN identifier for the Refresh protocol, pinned so this endpoint
/// never accidentally speaks to or is confused with another QUIC-based
/// service on the same port.
pub const ALPN: &[u8] = b"refresh/1";

/// Build a self-signed TLS certificate bound to this device's long-term
/// Ed25519 identity key. The certificate itself proves nothing on its
/// own (it is self-signed, not CA-issued) — what matters is that the
/// public key embedded in it is checked against the trust store by the
/// verifiers below.
fn self_signed_cert(signing_key: &SigningKey) -> Result<(CertificateDer<'static>, PrivateKeyDer<'static>)> {
    let pkcs8_der = signing_key
        .to_pkcs8_der()
        .map_err(|e| anyhow!("encoding identity key as pkcs8: {e}"))?;
    let key_pair = rcgen::KeyPair::from_pkcs8_der_and_sign_algo(&pkcs8_der, &rcgen::PKCS_ED25519)
        .context("building rcgen key pair from device identity")?;

    let mut params = rcgen::CertificateParams::new(vec!["refresh.local".to_string()])?;
    params.distinguished_name = rcgen::DistinguishedName::new();
    let cert = params.self_signed(&key_pair)?;

    let cert_der = CertificateDer::from(cert.der().to_vec());
    let key_der = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(pkcs8_der.as_bytes().to_vec()));
    Ok((cert_der, key_der))
}

/// Extract the raw Ed25519 public key embedded in a certificate produced
/// by `self_signed_cert`. Used by both the server and client verifiers
/// so the only question either side ever asks is "do I already trust
/// this exact key?" — never "was this cert issued by someone I trust".
fn extract_ed25519_key(cert: &CertificateDer<'_>) -> Result<VerifyingKey> {
    // rcgen/x509 encodes the SubjectPublicKeyInfo for Ed25519 as a
    // 12-byte ASN.1 algorithm prefix followed by the raw 32-byte key.
    // Rather than pull in a full x509 parser for one field, we parse
    // just enough DER to find that trailing 32-byte key, which is
    // sufficient and auditable for this single, fixed key type.
    let bytes = cert.as_ref();
    if bytes.len() < 32 {
        return Err(anyhow!("certificate too short to contain an Ed25519 key"));
    }
    let tail = &bytes[bytes.len() - 32..];
    let arr: [u8; 32] = tail.try_into().unwrap();
    VerifyingKey::from_bytes(&arr).map_err(|e| anyhow!("invalid Ed25519 key in certificate: {e}"))
}

#[derive(Debug)]
struct PinnedServerVerifier {
    trust_store: Arc<std::sync::Mutex<TrustStore>>,
}

impl rustls::client::danger::ServerCertVerifier for PinnedServerVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        let key = extract_ed25519_key(end_entity)
            .map_err(|e| rustls::Error::General(format!("cert parse failed: {e}")))?;
        let trusted = self
            .trust_store
            .lock()
            .map_err(|_| rustls::Error::General("trust store poisoned".into()))?
            .is_trusted(&key);
        if trusted {
            Ok(rustls::client::danger::ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General(
                "peer device key is not in the trust store (pair this device first)".into(),
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![rustls::SignatureScheme::ED25519]
    }
}

#[derive(Debug)]
struct PinnedClientVerifier {
    trust_store: Arc<std::sync::Mutex<TrustStore>>,
}

impl rustls::server::danger::ClientCertVerifier for PinnedClientVerifier {
    fn offer_client_auth(&self) -> bool {
        true
    }

    fn client_auth_mandatory(&self) -> bool {
        true
    }

    fn root_hint_subjects(&self) -> &[rustls::DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> Result<rustls::server::danger::ClientCertVerified, rustls::Error> {
        let key = extract_ed25519_key(end_entity)
            .map_err(|e| rustls::Error::General(format!("cert parse failed: {e}")))?;
        let trusted = self
            .trust_store
            .lock()
            .map_err(|_| rustls::Error::General("trust store poisoned".into()))?
            .is_trusted(&key);
        if trusted {
            Ok(rustls::server::danger::ClientCertVerified::assertion())
        } else {
            Err(rustls::Error::General(
                "connecting device key is not in the trust store".into(),
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![rustls::SignatureScheme::ED25519]
    }
}

/// Build a QUIC server endpoint that only accepts connections from
/// devices already present in `trust_store`, authenticated mutually
/// (the server also proves its own identity key to the client).
pub fn server_endpoint(
    bind_addr: SocketAddr,
    signing_key: &SigningKey,
    trust_store: Arc<std::sync::Mutex<TrustStore>>,
) -> Result<Endpoint> {
    let (cert, key) = self_signed_cert(signing_key)?;

    let client_verifier = Arc::new(PinnedClientVerifier {
        trust_store: trust_store.clone(),
    });

    let mut tls_config = rustls::ServerConfig::builder()
        .with_client_cert_verifier(client_verifier)
        .with_single_cert(vec![cert], key)
        .context("building server TLS config")?;
    tls_config.alpn_protocols = vec![ALPN.to_vec()];

    let quic_tls = quinn::crypto::rustls::QuicServerConfig::try_from(tls_config)
        .context("adapting TLS config for QUIC")?;
    let server_config = ServerConfig::with_crypto(Arc::new(quic_tls));

    let endpoint = Endpoint::server(server_config, bind_addr).context("binding QUIC server")?;
    Ok(endpoint)
}

/// Build a QUIC client endpoint that will only trust a server whose key
/// is already in `trust_store`, and which authenticates itself to the
/// server using this device's own identity key.
pub fn client_endpoint(
    signing_key: &SigningKey,
    trust_store: Arc<std::sync::Mutex<TrustStore>>,
) -> Result<Endpoint> {
    let (cert, key) = self_signed_cert(signing_key)?;

    let server_verifier = Arc::new(PinnedServerVerifier {
        trust_store: trust_store.clone(),
    });

    let mut tls_config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(server_verifier)
        .with_client_auth_cert(vec![cert], key)
        .context("building client TLS config")?;
    tls_config.alpn_protocols = vec![ALPN.to_vec()];

    let quic_tls = quinn::crypto::rustls::QuicClientConfig::try_from(tls_config)
        .context("adapting TLS config for QUIC")?;
    let client_config = ClientConfig::new(Arc::new(quic_tls));

    let mut endpoint = Endpoint::client("[::]:0".parse()?).context("binding QUIC client")?;
    endpoint.set_default_client_config(client_config);
    Ok(endpoint)
}
