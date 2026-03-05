pub mod keys;
pub mod signature;
pub mod aggregation;

// Re-export main types for easy access
pub use keys::{SecretKey, PublicKey, BlsSecretKeyExt, BlsPublicKeyExt};
pub use keys::{generate_secret_key, secret_to_public, pubkey_to_bytes, public_key_from_bytes};
pub use signature::{Signature, BlsSignerExt, BlsSignatureExt, signature_from_bytes};
pub use aggregation::{aggregate_signatures, verify_batch};

// Re-export AggregateSignature directly from blst for external use
pub use blst::min_sig::AggregateSignature;


