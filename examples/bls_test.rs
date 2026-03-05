use rand::Rng;
use triad::crypto::bls::keys::{BlsSecretKeyExt, SecretKey};
use triad::crypto::bls::signature::{BlsSignatureExt, BlsSignerExt};

fn main() {
    println!("Testing BLS BLAKE3 Signature...");

    let mut rng = rand::thread_rng();
    let sk = SecretKey::random(&mut rng);
    let pk = sk.public_key();
    let msg = b"test message for blake3 signature";

    println!("Signing message...");
    let sig = sk.sign_msg(msg);

    println!("Verifying signature...");
    let valid = sig.verify_msg(msg, &pk);

    if valid {
        println!("SUCCESS: Signature verified!");
    } else {
        println!("FAILURE: Signature verification failed!");
        std::process::exit(1);
    }

    let valid_wrong_msg = sig.verify_msg(b"wrong message", &pk);
    if !valid_wrong_msg {
        println!("SUCCESS: Wrong message rejected!");
    } else {
        println!("FAILURE: Wrong message accepted!");
        std::process::exit(1);
    }
}
