use curve25519_dalek::ristretto::RistrettoPoint;
use curve25519_dalek::scalar::Scalar;
use curve25519_dalek::traits::Identity;
use rayon::prelude::*;


/// Represents a participant in the threshold signature scheme.
#[derive(Debug, Clone)]
pub struct FrostParticipant {
    pub index: u32,
    pub secret_share: Scalar,
    pub public_key: RistrettoPoint,
    pub group_public_key: RistrettoPoint,
}

/// Represents a partial signature share from a participant.
#[derive(Debug, Clone)]
pub struct SignatureShare {
    pub index: u32,
    pub z_share: Scalar, // Response
    pub commitment: RistrettoPoint, // R_i
}

/// Aggregates partial signature shares into a final Schnorr signature.
/// R = sum(R_i)
/// z = sum(z_i)
pub fn aggregate_signature_shares(
    group_public_key: RistrettoPoint,
    shares: &[SignatureShare],
    message: &[u8],
) -> Result<super::schnorr::SchnorrSignature, &'static str> {
    if shares.is_empty() {
        return Err("No shares provided");
    }

    // 1. Compute Group Commitment R = sum(R_i)
    let R: RistrettoPoint = shares
        .par_iter()
        .map(|share| share.commitment)
        .reduce(|| RistrettoPoint::identity(), |a, b| a + b);

    // 2. Compute Challenge c = H(R || P || m)
    let mut hasher = blake3::Hasher::new();
    hasher.update(R.compress().as_bytes());
    hasher.update(group_public_key.compress().as_bytes());
    hasher.update(message);
    let c_hash = hasher.finalize();
    // Widen deterministically without heap concat
    let mut wide = [0u8; 64];
    wide[..32].copy_from_slice(c_hash.as_bytes());
    wide[32..].copy_from_slice(c_hash.as_bytes());
    let c = Scalar::from_bytes_mod_order_wide(&wide);

    // 3. Compute z = sum(z_i)
    let z: Scalar = shares
        .par_iter()
        .map(|share| share.z_share)
        .sum();

    // 4. Verify the aggregated signature equation: z * G == R + c * P
    // This is done by the verifier, but we construct the signature here.
    
    Ok(super::schnorr::SchnorrSignature {
        r: R.compress(),
        s: z,
    })
}

/// Verifies a batch of threshold signatures in parallel.
pub fn batch_verify_threshold(
    items: &[(RistrettoPoint, &[u8], super::schnorr::SchnorrSignature)],
) -> bool {
    items.par_iter().all(|(pk, msg, sig)| {
        super::schnorr::verify(pk, msg, sig)

    })
}

// Placeholder for DKG (Distributed Key Generation)
// This is a complex interactive protocol. For now, we provide the structure.
pub struct DistributedKeyGeneration {
    pub threshold: u32,
    pub participants: u32,
}


