use blst::min_sig::{Signature as BlstSignature, SecretKey, PublicKey};

pub type Signature = BlstSignature;

// Standard DST for BLS12-381 MinSig with SHA-256 and Proof of Possession
// This is the standard used by Ethereum 2.0 and others
pub const DST: &[u8] = b"BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_POP_";

pub trait BlsSignerExt {
    fn sign_msg(&self, message: &[u8]) -> Signature;
}

impl BlsSignerExt for SecretKey {
    fn sign_msg(&self, message: &[u8]) -> Signature {
        // Use standard blst sign method which uses SHA-256
        // This handles hash_to_curve, scalar multiplication, and cofactor clearing correctly
        self.sign(message, DST, &[])
    }
}

pub trait BlsSignatureExt {
    fn verify_msg(&self, message: &[u8], public_key: &PublicKey) -> bool;
    fn to_bytes(&self) -> Vec<u8>;
}

impl BlsSignatureExt for Signature {
    fn verify_msg(&self, message: &[u8], public_key: &PublicKey) -> bool {
        // Use standard blst verify method
        // true for group check, true for pk validation
        let result = self.verify(true, message, DST, &[], public_key, true);
        result == blst::BLST_ERROR::BLST_SUCCESS
    }

    fn to_bytes(&self) -> Vec<u8> {
        self.to_bytes().to_vec()
    }
}

// Helper for signature deserialization
pub fn signature_from_bytes(bytes: &[u8]) -> Result<Signature, String> {
    Signature::from_bytes(bytes).map_err(|e| format!("Failed to deserialize Signature: {:?}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::bls::keys::BlsSecretKeyExt;
    
    #[test]
    fn test_sign_verify_standard() {
        let mut rng = rand::thread_rng();
        let sk = SecretKey::random(&mut rng);
        let pk = sk.public_key();
        let msg = b"test message for standard signature";
        
        let sig = sk.sign(msg);
        let valid = sig.verify(msg, &pk);
        
        assert!(valid, "Signature verification failed");
        
        let valid_wrong_msg = sig.verify(b"wrong message", &pk);
        assert!(!valid_wrong_msg, "Signature verified for wrong message");
    }
}


