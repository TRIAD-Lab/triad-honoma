use curve25519_dalek::scalar::Scalar;

use curve25519_dalek::ristretto::{RistrettoPoint, CompressedRistretto};
use curve25519_dalek::constants::RISTRETTO_BASEPOINT_POINT;
use blake3::Hasher;
use super::keys::{RistrettoKeypair, RistrettoPublicKey};

#[derive(Clone, Debug)]
pub struct RistrettoSignature {
    pub r_comp: CompressedRistretto,
    pub scalar: Scalar,
}

impl RistrettoSignature {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(64);
        bytes.extend_from_slice(self.r_comp.as_bytes());
        bytes.extend_from_slice(self.scalar.as_bytes());
        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() != 64 {
            return Err("Invalid signature length".to_string());
        }
        let r_comp = CompressedRistretto::from_slice(&bytes[0..32]).map_err(|_| "Invalid R")?;
        let scalar = Scalar::from_bytes_mod_order(bytes[32..64].try_into().unwrap());
        Ok(Self { r_comp, scalar })
    }
}

pub fn sign(keypair: &RistrettoKeypair, message: &[u8]) -> RistrettoSignature {
    // 1. Deterministic nonce r = H(secret, message)
    let mut hasher = Hasher::new();
    hasher.update(keypair.secret.as_bytes());
    hasher.update(message);
    let h = hasher.finalize();
    let mut wide = [0u8; 64];
    wide[..32].copy_from_slice(h.as_bytes());
    wide[32..].copy_from_slice(h.as_bytes());
    let r = Scalar::from_bytes_mod_order_wide(&wide);
    
    // 2. R = r * G
    let r_point = RISTRETTO_BASEPOINT_POINT * r;
    let r_comp = r_point.compress();
    
    // 3. e = H(R, P, m)
    let mut hasher = Hasher::new();
    hasher.update(r_comp.as_bytes());
    hasher.update(keypair.public.compress().as_bytes());
    hasher.update(message);
    let h = hasher.finalize();
    let mut wide = [0u8; 64];
    wide[..32].copy_from_slice(h.as_bytes());
    wide[32..].copy_from_slice(h.as_bytes());
    let e = Scalar::from_bytes_mod_order_wide(&wide);
    
    // 4. s = r + e * x
    let scalar = r + e * keypair.secret;
    
    RistrettoSignature { r_comp, scalar }
}

pub fn verify(public_key: &RistrettoPublicKey, message: &[u8], signature: &RistrettoSignature) -> bool {
    // 1. Decompress R
    let r_point = match signature.r_comp.decompress() {
        Some(p) => p,
        None => return false,
    };
    
    // 2. e = H(R, P, m) via blake3 (deterministic challenge)
    let mut hasher = Hasher::new();
    hasher.update(signature.r_comp.as_bytes());
    hasher.update(public_key.0.compress().as_bytes());
    hasher.update(message);
    let h = hasher.finalize();
    let mut wide = [0u8; 64];
    wide[..32].copy_from_slice(h.as_bytes());
    wide[32..].copy_from_slice(h.as_bytes());
    let e = Scalar::from_bytes_mod_order_wide(&wide);
    
    // 3. Check s * G == R + e * P
    let scalar_times_base = RISTRETTO_BASEPOINT_POINT * signature.scalar;
    let r_plus_ep = r_point + (public_key.0 * e);
    
    scalar_times_base == r_plus_ep
}


