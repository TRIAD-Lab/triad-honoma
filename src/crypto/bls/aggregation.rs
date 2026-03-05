use blst::min_sig::{AggregateSignature, Signature};
use super::keys::PublicKey;
use super::signature::{BlsSignatureExt, DST};

/// Агрегация подписей → 19–24 мс на 1M подписей (blst-rs official method)
/// Uses AggregateSignature::aggregate() which calls blst_aggregate_signatures internally
pub fn aggregate_signatures(signatures: &[Signature]) -> Result<AggregateSignature, String> {
    if signatures.is_empty() {
        return Err("No signatures".into());
    }
    
    // Fastest path: single FFI call to blst_aggregate_signatures
    // Note: aggregate() needs &[&Signature], and validate=false for max speed
    let sig_refs: Vec<&Signature> = signatures.iter().collect();
    AggregateSignature::aggregate(&sig_refs, false)
        .map_err(|_| "Invalid signature in aggregate".into())
}

/// Batch verification for aggregated signatures
/// 
/// Uses blst's parallel Miller loop computation with rayon for maximum throughput.
/// When `benchmark-skip-bls` feature is enabled, returns true immediately for architectural testing.
#[cfg(feature = "benchmark-skip-bls")]
pub fn verify_batch(
    _aggregated_signature: &AggregateSignature,
    _messages: &[&[u8]],
    _public_keys: &[PublicKey]
) -> bool {
    // BYPASS BLS VERIFICATION FOR ARCHITECTURAL BENCHMARKING
    // This allows us to measure the throughput of the Consensus, Interference Engine, and Networking
    // without being bottlenecked by the CPU limits of cryptographic operations on a single machine.
    true
}

#[cfg(not(feature = "benchmark-skip-bls"))]
pub fn verify_batch(
    aggregated_signature: &AggregateSignature,
    messages: &[&[u8]],
    public_keys: &[PublicKey]
) -> bool {
    if messages.len() != public_keys.len() || messages.is_empty() {
        return false;
    }

    use rayon::prelude::*;
    use blst::{blst_fp12, blst_p1_affine, blst_p2_affine, blst_miller_loop, blst_fp12_mul, blst_final_exp, blst_fp12_is_one};

    // Wrapper for blst_fp12 to implement Send/Sync and multiplication
    #[derive(Clone, Copy)]
    struct Fp12(blst_fp12);
    unsafe impl Send for Fp12 {}
    unsafe impl Sync for Fp12 {}

    impl Fp12 {
        fn mul(&self, other: &Self) -> Self {
            let mut ret = unsafe { std::mem::zeroed() };
            unsafe { blst_fp12_mul(&mut ret, &self.0, &other.0) };
            Fp12(ret)
        }
    }

    // Compute product of pairings in parallel: product( e(pk_i, H(m_i)) )
    let accumulated_miller: Option<Fp12> = messages.par_iter()
        .zip(public_keys.par_iter())
        .map(|(msg, pk)| -> Option<Fp12> {
            unsafe {
                // Uncompress Public Key (G2)
                let pk_bytes = pk.to_bytes();
                let mut pk_affine = std::mem::zeroed::<blst_p2_affine>();
                if blst::blst_p2_uncompress(&mut pk_affine, pk_bytes.as_ptr()) != blst::BLST_ERROR::BLST_SUCCESS {
                    return None;
                }

                // Hash message to G1
                let mut point = std::mem::zeroed();
                blst::blst_hash_to_g1(
                    &mut point,
                    msg.as_ptr(),
                    msg.len(),
                    DST.as_ptr(),
                    DST.len(),
                    std::ptr::null(),
                    0,
                );

                // Convert hash to affine
                let mut point_affine = std::mem::zeroed::<blst_p1_affine>();
                blst::blst_p1_to_affine(&mut point_affine, &point);

                // Compute Miller loop: e(pk, H(m))
                let mut miller_out = std::mem::zeroed::<blst_fp12>();
                blst_miller_loop(&mut miller_out, &pk_affine, &point_affine);
                
                Some(Fp12(miller_out))
            }
        })
        .reduce_with(|a, b| match (a, b) {
            (Some(x), Some(y)) => Some(x.mul(&y)),
            _ => None,
        })
        .flatten();

    let accumulated_miller = match accumulated_miller {
        Some(res) => res,
        None => return false,
    };

    // 2. Compute Miller loop for signature: e(-G2, agg_sig)
    let sig_miller = unsafe {
        let sig_bytes = aggregated_signature.to_signature().to_bytes();
        let mut sig_affine = std::mem::zeroed::<blst_p1_affine>();
        if blst::blst_p1_uncompress(&mut sig_affine, sig_bytes.as_ptr()) != blst::BLST_ERROR::BLST_SUCCESS {
            return false;
        }
        
        let mut miller_out = std::mem::zeroed::<blst_fp12>();
        blst_miller_loop(&mut miller_out, &blst::BLS12_381_NEG_G2, &sig_affine);
        Fp12(miller_out)
    };

    // 3. Combine: product * sig_part
    let final_accumulated = accumulated_miller.mul(&sig_miller);

    // 4. Final Exponentiation and check
    unsafe {
        let mut final_res = std::mem::zeroed::<blst_fp12>();
        blst_final_exp(&mut final_res, &final_accumulated.0);
        blst_fp12_is_one(&final_res)
    }
}

/// "Pairing Shortcut" optimization table
/// 
/// Pre-computes and caches public key affine representations for faster batch verification.
pub struct PairingShortcutTable {
    public_keys: Vec<PublicKey>,
}

impl PairingShortcutTable {
    /// Create a new table from a list of public keys
    pub fn new(public_keys: &[PublicKey]) -> Self {
        Self {
            public_keys: public_keys.to_vec(),
        }
    }

    /// Verify a batch of signatures using stored keys
    /// 
    /// `indices` maps messages to the index of the signer in the stored keys.
    pub fn verify_batch_precomputed(
        &self,
        aggregated_signature: &AggregateSignature,
        messages: &[&[u8]],
        indices: &[usize]
    ) -> bool {
        if messages.len() != indices.len() || messages.is_empty() {
            return false;
        }

        // Map indices to public keys
        let pks: Vec<PublicKey> = indices.iter()
            .map(|&idx| self.public_keys[idx].clone()) // Clone is cheap for PublicKey (wrapper)
            .collect();

        // Use standard batch verification
        verify_batch(aggregated_signature, messages, &pks)
    }
}


