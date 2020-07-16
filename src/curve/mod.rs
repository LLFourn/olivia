pub mod ed25519;
pub mod secp256k1;
use crate::seed::Seed;
pub use ed25519::Ed25519;
pub use secp256k1::Secp256k1;

pub trait Curve {
    type SchnorrScalar: PartialEq + Clone + core::fmt::Debug;
    type PublicKey: PartialEq + Clone + core::fmt::Debug;
    type KeyPair: Into<Self::PublicKey>;
    type SchnorrSignature: PartialEq + Clone + core::fmt::Debug;

    fn derive_keypair(seed: &Seed) -> Self::KeyPair;
    fn derive_nonce_keypair(seed: &Seed) -> Self::KeyPair;
    fn reveal_signature_s(
        signing_key: &Self::KeyPair,
        nonce_key: &Self::KeyPair,
        message: &[u8],
    ) -> Self::SchnorrScalar;
    fn signature_from_scalar_and_nonce(
        scalar: Self::SchnorrScalar,
        nonce: Self::PublicKey,
    ) -> Self::SchnorrSignature;
    fn verify_signature(
        public_key: &Self::PublicKey,
        message: &[u8],
        sig: &Self::SchnorrSignature,
    ) -> bool;
}
