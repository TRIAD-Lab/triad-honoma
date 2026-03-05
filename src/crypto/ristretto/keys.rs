use curve25519_dalek::ristretto::{RistrettoPoint, CompressedRistretto};
use curve25519_dalek::scalar::Scalar;
use curve25519_dalek::constants::RISTRETTO_BASEPOINT_POINT;
use rand::{Rng, CryptoRng};

pub struct RistrettoSecretKey(pub Scalar);
pub struct RistrettoPublicKey(pub RistrettoPoint);

#[derive(Clone)]
pub struct RistrettoKeypair {
    pub secret: Scalar,
    pub public: RistrettoPoint,
}

impl RistrettoKeypair {
    pub fn generate<R: Rng + CryptoRng>(rng: &mut R) -> Self {
        let mut bytes = [0u8; 64];
        rng.fill_bytes(&mut bytes);
        
        // Standard way to generate scalar from bytes (clamping not strictly needed for Ristretto but good practice for uniformity if reusing Ed25519 logic, though here we just want a uniform scalar)
        // For Ristretto, we can just pick a random scalar.
        let secret = Scalar::from_bytes_mod_order_wide(&bytes);
        let public = RISTRETTO_BASEPOINT_POINT * secret;
        
        Self { secret, public }
    }
    
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() != 64 {
            return Err("Invalid keypair length".to_string());
        }
        let secret = Scalar::from_bytes_mod_order(bytes[0..32].try_into().unwrap());
        let public_compressed = CompressedRistretto::from_slice(&bytes[32..64])
            .map_err(|_| "Invalid public key bytes")?;
        let public = public_compressed.decompress().ok_or("Invalid public key point")?;
        
        Ok(Self { secret, public })
    }
    
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(64);
        bytes.extend_from_slice(self.secret.as_bytes());
        bytes.extend_from_slice(self.public.compress().as_bytes());
        bytes
    }
}


