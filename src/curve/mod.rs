//pub mod ed25519;
pub mod secp256k1;
//pub use ed25519::Ed25519;
pub use secp256k1::Secp256k1;
use crate::seed::Seed;


pub type CurveImpl = Secp256k1;
pub type PublicKey = secp256k1::PublicKey;
pub type PublicNonce = secp256k1::PublicNonce;
pub type SchnorrScalar = secp256k1::SchnorrScalar;
pub type SchnorrSignature = secp256k1::SchnorrSignature;

pub trait DeriveKeyPair: olivia_core::Curve {
    fn derive_keypair(seed: &Seed) -> Self::KeyPair;
    fn derive_nonce_keypair(seed: &Seed) -> Self::NonceKeyPair;
}
