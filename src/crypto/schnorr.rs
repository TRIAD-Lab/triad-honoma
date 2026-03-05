/// Schnorr signatures on Ristretto255 with Blake3 hashing
/// 
/// High-performance signature scheme for TRIAD consensus:
/// - 10-100x faster than BLS12-381
/// - 64-byte signatures (vs 96 bytes BLS)
/// - Batch verification in O(N)
/// - Blake3 for transcript hashing

use curve25519_dalek::{
    ristretto::{CompressedRistretto, RistrettoPoint},
    scalar::Scalar,
    traits::Identity,
    constants::RISTRETTO_BASEPOINT_POINT,
};
use blake3::Hasher;
use rand_core::{RngCore, OsRng};

/// Schnorr keypair
#[derive(Clone, Debug)]
pub struct SchnorrKeypair {
    pub secret: Scalar,
    pub public: RistrettoPoint,
}

/// Schnorr signature: (R, s) where R is commitment, s is response
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchnorrSignature {
    pub r: CompressedRistretto,  // 32 bytes
    pub s: Scalar,                // 32 bytes
}

impl SchnorrSignature {
    /// Serialize to 64 bytes
    pub fn to_bytes(&self) -> [u8; 64] {
        let mut bytes = [0u8; 64];
        bytes[..32].copy_from_slice(self.r.as_bytes());
        bytes[32..].copy_from_slice(self.s.as_bytes());
        bytes
    }

    /// Deserialize from 64 bytes
    pub fn from_bytes(bytes: &[u8; 64]) -> Option<Self> {
        let r = CompressedRistretto::from_slice(&bytes[..32]).ok()?;
        // CtOption -> Option without panic
        let s = Option::<Scalar>::from(Scalar::from_canonical_bytes(bytes[32..].try_into().ok()?))?;
        Some(Self { r, s })
    }
}

/// Generate a new Schnorr keypair
pub fn generate_keypair() -> SchnorrKeypair {
    let mut rng = OsRng;
    let mut secret_bytes = [0u8; 64];
    rng.fill_bytes(&mut secret_bytes);
    let secret = Scalar::from_bytes_mod_order_wide(&secret_bytes);
    let public = secret * RISTRETTO_BASEPOINT_POINT;
    
    SchnorrKeypair { secret, public }
}

/// Generate keypair from seed (deterministic, for testing)
pub fn generate_keypair_from_seed(seed: &[u8; 32]) -> SchnorrKeypair {
    let secret = Scalar::from_bytes_mod_order(*seed);
    let public = secret * RISTRETTO_BASEPOINT_POINT;
    
    SchnorrKeypair { secret, public }
}

/// Sign a message using Schnorr signature scheme
/// 
/// Formula:
/// 1. r = random nonce, R = r·G
/// 2. c = H(R || P || m)  // challenge
/// 3. s = r + c·x         // response
/// Signature: (R, s)
pub fn sign(keypair: &SchnorrKeypair, message: &[u8]) -> SchnorrSignature {
    let mut rng = OsRng;
    let mut nonce_bytes = [0u8; 64];
    rng.fill_bytes(&mut nonce_bytes);
    let nonce = Scalar::from_bytes_mod_order_wide(&nonce_bytes);
    
    sign_with_nonce(keypair, message, nonce)
}

/// Sign with explicit nonce (for testing/deterministic signing)
fn sign_with_nonce(keypair: &SchnorrKeypair, message: &[u8], nonce: Scalar) -> SchnorrSignature {
    // R = r·G
    let r_point = nonce * RISTRETTO_BASEPOINT_POINT;
    let r = r_point.compress();
    
    // c = H(R || P || m)
    let challenge = compute_challenge(&r, &keypair.public.compress(), message);
    
    // s = r + c·x
    let s = nonce + challenge * keypair.secret;
    
    SchnorrSignature { r, s }
}

/// Verify a Schnorr signature
/// 
/// Verification:
/// 1. Parse (R, s)
/// 2. c = H(R || P || m)
/// 3. Check: s·G == R + c·P
pub fn verify(public_key: &RistrettoPoint, message: &[u8], signature: &SchnorrSignature) -> bool {
    // Decompress R
    let r_point = match signature.r.decompress() {
        Some(p) => p,
        None => return false,
    };
    
    // c = H(R || P || m)
    let challenge = compute_challenge(&signature.r, &public_key.compress(), message);
    
    // Check: s·G == R + c·P
    let lhs = signature.s * RISTRETTO_BASEPOINT_POINT;
    let rhs = r_point + challenge * public_key;
    
    lhs == rhs
}

/// Batch verify multiple Schnorr signatures (FAST!)
/// 
/// Batch verification formula:
/// Σ(w_i · s_i)·G == Σ(w_i · R_i) + Σ(w_i · c_i · P_i)
/// 
/// where w_i are random weights to prevent forgery
pub fn verify_batch(
    public_keys: &[RistrettoPoint],
    messages: &[&[u8]],
    signatures: &[SchnorrSignature],
) -> bool {
    let n = public_keys.len();
    
    if n != messages.len() || n != signatures.len() {
        return false;
    }
    
    if n == 0 {
        return true;
    }
    
    // Special case: single signature
    if n == 1 {
        return verify(&public_keys[0], messages[0], &signatures[0]);
    }
    
    // Generate random weights
    let mut rng = OsRng;
    let weights: Vec<Scalar> = (0..n).map(|_| {
        let mut bytes = [0u8; 64];
        rng.fill_bytes(&mut bytes);
        Scalar::from_bytes_mod_order_wide(&bytes)
    }).collect();
    
    // Compute Σ(w_i · s_i)
    let lhs_scalar: Scalar = weights.iter()
        .zip(signatures.iter())
        .map(|(w, sig)| w * sig.s)
        .sum();
    let lhs = lhs_scalar * RISTRETTO_BASEPOINT_POINT;
    
    // Compute Σ(w_i · R_i) + Σ(w_i · c_i · P_i)
    let mut rhs = RistrettoPoint::identity();
    
    for i in 0..n {
        // Decompress R_i
        let r_point = match signatures[i].r.decompress() {
            Some(p) => p,
            None => return false,
        };
        
        // c_i = H(R_i || P_i || m_i)
        let challenge = compute_challenge(
            &signatures[i].r,
            &public_keys[i].compress(),
            messages[i],
        );
        
        // Add w_i · R_i
        rhs += weights[i] * r_point;
        
        // Add w_i · c_i · P_i
        rhs += (weights[i] * challenge) * public_keys[i];
    }
    
    lhs == rhs
}

/// Compute challenge: c = H(R || P || m)
/// Uses Blake3 for fast hashing
fn compute_challenge(
    r: &CompressedRistretto,
    public_key: &CompressedRistretto,
    message: &[u8],
) -> Scalar {
    let mut hasher = Hasher::new();
    hasher.update(b"TRIAD-Schnorr-v1"); // Domain separator
    hasher.update(r.as_bytes());
    hasher.update(public_key.as_bytes());
    hasher.update(message);
    
    let hash = hasher.finalize();
    Scalar::from_bytes_mod_order(*hash.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sign_verify_roundtrip() {
        let keypair = generate_keypair();
        let message = b"Hello, TRIAD!";
        
        let signature = sign(&keypair, message);
        assert!(verify(&keypair.public, message, &signature));
    }

    #[test]
    fn test_sign_verify_wrong_message() {
        let keypair = generate_keypair();
        let message = b"Hello, TRIAD!";
        let wrong_message = b"Goodbye, TRIAD!";
        
        let signature = sign(&keypair, message);
        assert!(!verify(&keypair.public, wrong_message, &signature));
    }

    #[test]
    fn test_sign_verify_wrong_pubkey() {
        let keypair1 = generate_keypair();
        let keypair2 = generate_keypair();
        let message = b"Hello, TRIAD!";
        
        let signature = sign(&keypair1, message);
        assert!(!verify(&keypair2.public, message, &signature));
    }

    #[test]
    fn test_signature_serialization() {
        let keypair = generate_keypair();
        let message = b"Test message";
        
        let signature = sign(&keypair, message);
        let bytes = signature.to_bytes();
        let signature2 = SchnorrSignature::from_bytes(&bytes).unwrap();
        
        assert_eq!(signature, signature2);
        assert!(verify(&keypair.public, message, &signature2));
    }

    #[test]
    fn test_batch_verify_single() {
        let keypair = generate_keypair();
        let message = b"Single message";
        let signature = sign(&keypair, message);
        
        assert!(verify_batch(
            &[keypair.public],
            &[message.as_ref()],
            &[signature],
        ));
    }

    #[test]
    fn test_batch_verify_multiple() {
        let n = 10;
        let keypairs: Vec<_> = (0..n).map(|_| generate_keypair()).collect();
        let messages: Vec<Vec<u8>> = (0..n)
            .map(|i| format!("Message {}", i).into_bytes())
            .collect();
        let signatures: Vec<_> = keypairs.iter()
            .zip(messages.iter())
            .map(|(kp, msg)| sign(kp, msg))
            .collect();
        
        let publics: Vec<_> = keypairs.iter().map(|kp| kp.public).collect();
        let msg_refs: Vec<&[u8]> = messages.iter().map(|m| m.as_slice()).collect();
        
        assert!(verify_batch(&publics, &msg_refs, &signatures));
    }

    #[test]
    fn test_batch_verify_one_invalid() {
        let n = 10;
        let keypairs: Vec<_> = (0..n).map(|_| generate_keypair()).collect();
        let mut messages: Vec<Vec<u8>> = (0..n)
            .map(|i| format!("Message {}", i).into_bytes())
            .collect();
        let signatures: Vec<_> = keypairs.iter()
            .zip(messages.iter())
            .map(|(kp, msg)| sign(kp, msg))
            .collect();
        
        // Tamper with one message
        messages[5] = b"Tampered message".to_vec();
        
        let publics: Vec<_> = keypairs.iter().map(|kp| kp.public).collect();
        let msg_refs: Vec<&[u8]> = messages.iter().map(|m| m.as_slice()).collect();
        
        assert!(!verify_batch(&publics, &msg_refs, &signatures));
    }

    #[test]
    fn test_deterministic_signing() {
        let seed = [42u8; 32];
        let keypair1 = generate_keypair_from_seed(&seed);
        let keypair2 = generate_keypair_from_seed(&seed);
        
        assert_eq!(keypair1.secret, keypair2.secret);
        assert_eq!(keypair1.public, keypair2.public);
    }
}


