use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    ChaCha20Poly1305, Nonce,
};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use hkdf::Hkdf;
use rand::rngs::OsRng;
use sha2::Sha256;
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret as X25519Secret};

#[derive(Clone)]
pub struct DeviceIdentity {
    pub device_id: String,
    pub signing_key: SigningKey,
    pub verifying_key: VerifyingKey,
}

impl DeviceIdentity {
    pub fn generate() -> Self {
        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        let verifying_key = signing_key.verifying_key();
        let device_id = Self::derive_device_id(&verifying_key);
        Self {
            device_id,
            signing_key,
            verifying_key,
        }
    }

    pub fn from_secret_hex(secret_hex: &str) -> Result<Self, String> {
        let bytes = hex::decode(secret_hex).map_err(|e| e.to_string())?;
        let arr: [u8; 32] = bytes
            .try_into()
            .map_err(|_| "Invalid Ed25519 secret key length".to_string())?;
        let signing_key = SigningKey::from_bytes(&arr);
        let verifying_key = signing_key.verifying_key();
        let device_id = Self::derive_device_id(&verifying_key);
        Ok(Self {
            device_id,
            signing_key,
            verifying_key,
        })
    }

    pub fn secret_hex(&self) -> String {
        hex::encode(self.signing_key.to_bytes())
    }

    pub fn public_hex(&self) -> String {
        hex::encode(self.verifying_key.to_bytes())
    }

    pub fn derive_device_id(verifying_key: &VerifyingKey) -> String {
        let hash = blake3::hash(verifying_key.as_bytes());
        let hex_str = hash.to_hex().to_uppercase();
        format!(
            "RAY-{}-{}-{}",
            &hex_str[0..4],
            &hex_str[4..8],
            &hex_str[8..12]
        )
    }

    pub fn verify_peer_identity(
        claimed_device_id: &str,
        ed25519_pubkey_hex: &str,
        message: &[u8],
        signature_hex: &str,
    ) -> Result<VerifyingKey, String> {
        let pub_bytes = hex::decode(ed25519_pubkey_hex).map_err(|e| e.to_string())?;
        let pub_arr: [u8; 32] = pub_bytes
            .try_into()
            .map_err(|_| "Invalid peer public key length".to_string())?;
        let verifying_key = VerifyingKey::from_bytes(&pub_arr).map_err(|e| e.to_string())?;

        let expected_id = Self::derive_device_id(&verifying_key);
        if expected_id != claimed_device_id {
            return Err(format!(
                "Device ID mismatch: claimed {claimed_device_id}, derived {expected_id}"
            ));
        }

        let sig_bytes = hex::decode(signature_hex).map_err(|e| e.to_string())?;
        let sig = Signature::from_slice(&sig_bytes).map_err(|e| e.to_string())?;
        verifying_key
            .verify(message, &sig)
            .map_err(|e| format!("Ed25519 signature verification failed: {e}"))?;

        Ok(verifying_key)
    }

    pub fn sign_message(&self, message: &[u8]) -> String {
        let sig = self.signing_key.sign(message);
        hex::encode(sig.to_bytes())
    }
}

pub struct EphemeralDh {
    secret: X25519Secret,
    pub public: X25519PublicKey,
}

impl EphemeralDh {
    pub fn generate() -> Self {
        let secret = X25519Secret::random_from_rng(OsRng);
        let public = X25519PublicKey::from(&secret);
        Self { secret, public }
    }

    pub fn public_hex(&self) -> String {
        hex::encode(self.public.as_bytes())
    }

    pub fn derive_session_key(
        self,
        peer_x25519_pub_hex: &str,
        session_id_bytes: &[u8; 16],
    ) -> Result<[u8; 32], String> {
        let peer_bytes = hex::decode(peer_x25519_pub_hex).map_err(|e| e.to_string())?;
        let peer_arr: [u8; 32] = peer_bytes
            .try_into()
            .map_err(|_| "Invalid X25519 public key length".to_string())?;
        let peer_pub = X25519PublicKey::from(peer_arr);
        let shared_secret = self.secret.diffie_hellman(&peer_pub);

        let hk = Hkdf::<Sha256>::new(Some(session_id_bytes), shared_secret.as_bytes());
        let mut session_key = [0u8; 32];
        hk.expand(b"raydrop-v1-chunk-key", &mut session_key)
            .map_err(|e| e.to_string())?;
        Ok(session_key)
    }
}

/// Deterministic 12-byte nonce: file_id (4B BigEndian) || offset (8B BigEndian)
pub fn build_chunk_nonce(file_id: u32, offset: u64) -> [u8; 12] {
    let mut nonce = [0u8; 12];
    nonce[0..4].copy_from_slice(&file_id.to_be_bytes());
    nonce[4..12].copy_from_slice(&offset.to_be_bytes());
    nonce
}

pub fn encrypt_chunk(
    session_key: &[u8; 32],
    file_id: u32,
    offset: u64,
    header_aad: &[u8],
    plaintext: &[u8],
) -> Result<Vec<u8>, String> {
    let cipher = ChaCha20Poly1305::new(session_key.into());
    let nonce_bytes = build_chunk_nonce(file_id, offset);
    let nonce = Nonce::from_slice(&nonce_bytes);
    cipher
        .encrypt(
            nonce,
            Payload {
                msg: plaintext,
                aad: header_aad,
            },
        )
        .map_err(|e| format!("Chunk encryption failed: {e}"))
}

pub fn decrypt_chunk(
    session_key: &[u8; 32],
    file_id: u32,
    offset: u64,
    header_aad: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, String> {
    let cipher = ChaCha20Poly1305::new(session_key.into());
    let nonce_bytes = build_chunk_nonce(file_id, offset);
    let nonce = Nonce::from_slice(&nonce_bytes);
    cipher
        .decrypt(
            nonce,
            Payload {
                msg: ciphertext,
                aad: header_aad,
            },
        )
        .map_err(|e| format!("Chunk decryption failed (tampered or wrong key): {e}"))
}

