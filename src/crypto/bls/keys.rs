use blst::min_sig::{SecretKey as BlstSecretKey, PublicKey as BlstPublicKey};
use rand::Rng;

// Re-export types for external use
pub type SecretKey = BlstSecretKey;
pub type PublicKey = BlstPublicKey;

/// Extension trait for SecretKey to provide OOP-like methods
pub trait BlsSecretKeyExt {
    fn random<R: Rng + ?Sized>(rng: &mut R) -> Self;
    fn public_key(&self) -> PublicKey;
    fn to_bytes(&self) -> Vec<u8>;
    fn from_bytes(bytes: &[u8]) -> Result<Self, String> where Self: Sized;
}

impl BlsSecretKeyExt for SecretKey {
    fn random<R: Rng + ?Sized>(rng: &mut R) -> Self {
        let mut ikm = [0u8; 32];
        rng.fill_bytes(&mut ikm);
        SecretKey::key_gen(&ikm, &[]).expect("Failed to generate BLS secret key")
    }

    fn public_key(&self) -> PublicKey {
        self.sk_to_pk()
    }

    fn to_bytes(&self) -> Vec<u8> {
        self.to_bytes().to_vec()
    }

    fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        SecretKey::from_bytes(bytes).map_err(|e| format!("Failed to deserialize SecretKey: {:?}", e))
    }
}

/// Extension trait for PublicKey
pub trait BlsPublicKeyExt {
    fn to_bytes(&self) -> Vec<u8>;
    fn from_bytes(bytes: &[u8]) -> Result<Self, String> where Self: Sized;
}

impl BlsPublicKeyExt for PublicKey {
    fn to_bytes(&self) -> Vec<u8> {
        self.to_bytes().to_vec()
    }

    fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        PublicKey::from_bytes(bytes).map_err(|e| format!("Failed to deserialize PublicKey: {:?}", e))
    }
}

// Helper functions for compatibility
pub fn generate_secret_key<R: Rng + ?Sized>(rng: &mut R) -> SecretKey {
    SecretKey::random(rng)
}

pub fn secret_to_public(secret: &SecretKey) -> PublicKey {
    secret.public_key()
}

pub fn pubkey_to_bytes(pk: &PublicKey) -> Vec<u8> {
    pk.to_bytes().to_vec()
}

pub fn public_key_from_bytes(bytes: &[u8]) -> Result<PublicKey, String> {
    PublicKey::from_bytes(bytes).map_err(|e| format!("Failed to deserialize PublicKey: {:?}", e))
}


