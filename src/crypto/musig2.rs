use curve25519_dalek::ristretto::RistrettoPoint;
use curve25519_dalek::scalar::Scalar;
use curve25519_dalek::traits::Identity;
use rayon::prelude::*;


/// Aggregates a list of public keys into a single aggregate public key using MuSig2-style linear combination.
/// L = H(P_1 || ... || P_n)
/// X_agg = sum( a_i * P_i ) where a_i = H(L || P_i)
pub fn aggregate_public_keys(public_keys: &[RistrettoPoint]) -> RistrettoPoint {
    if public_keys.is_empty() {
        return RistrettoPoint::identity();
    }

    // 1. Compute L = H(P_1 || ... || P_n)
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"MuSig2_Aggregation_Coefficient"); // Domain separation
    for pk in public_keys {
        hasher.update(pk.compress().as_bytes());
    }
    let l_hash = hasher.finalize();

    // 2. Compute X_agg = sum( a_i * P_i ) in parallel
    let aggregate_key: RistrettoPoint = public_keys
        .par_iter()
        .map(|pk| {
            let mut h = blake3::Hasher::new();
            h.update(l_hash.as_bytes());
            h.update(pk.compress().as_bytes());
            let a_i_hash = h.finalize();

            // Convert hash to Scalar (avoid alloc/concat; widen deterministically)
            let mut wide = [0u8; 64];
            wide[..32].copy_from_slice(a_i_hash.as_bytes());
            wide[32..].copy_from_slice(a_i_hash.as_bytes());
            let a_i = Scalar::from_bytes_mod_order_wide(&wide);
            
            pk * a_i
        })
        .reduce(|| RistrettoPoint::identity(), |a, b| a + b);

    aggregate_key
}

/// Verifies a batch of aggregated signatures.
/// Each item is (Aggregate PubKey, Message, Signature).
/// Uses Rayon for parallel verification.
pub fn batch_verify_aggregated(
    items: &[(RistrettoPoint, &[u8], super::schnorr::SchnorrSignature)],
) -> bool {
    items.par_iter().all(|(agg_pk, msg, sig)| {
        super::schnorr::verify(agg_pk, msg, sig)

    })
}

// Placeholder for full MuSig2 signing protocol (interactive)
// For now, we focus on Key Aggregation and Verification as requested.
pub struct MuSig2Context {
    pub key_agg_ctx: [u8; 32],
    // ...
}


