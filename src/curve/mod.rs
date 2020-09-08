//pub mod ed25519;
pub mod secp256k1;
//pub use ed25519::Ed25519;
use crate::seed::Seed;
pub use secp256k1::Secp256k1;

pub type SchnorrImpl = Secp256k1;
pub type PublicKey = secp256k1::PublicKey;
pub type PublicNonce = secp256k1::PublicNonce;
pub type SigScalar = secp256k1::SigScalar;
pub type Signature = secp256k1::Signature;

pub trait DeriveKeyPair: olivia_core::Schnorr {
    fn derive_keypair(seed: &Seed) -> Self::KeyPair;
    fn derive_nonce_keypair(seed: &Seed) -> Self::NonceKeyPair;
}
