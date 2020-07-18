pub mod ed25519;
pub mod secp256k1;
use crate::seed::Seed;
pub use ed25519::Ed25519;
pub use secp256k1::Secp256k1;

pub trait Curve {
    type SchnorrScalar: PartialEq + Clone + core::fmt::Debug;
    type PublicKey: PartialEq + Clone + core::fmt::Debug;
    type PublicNonce: PartialEq + Clone + core::fmt::Debug;
    type KeyPair: Into<Self::PublicKey>;
    type NonceKeyPair: Into<Self::PublicNonce>;
    type SchnorrSignature: PartialEq + Clone + core::fmt::Debug;

    fn derive_keypair(seed: &Seed) -> Self::KeyPair;
    fn derive_nonce_keypair(seed: &Seed) -> Self::NonceKeyPair;
    fn reveal_signature_s(
        signing_key: &Self::KeyPair,
        nonce_key: &Self::NonceKeyPair,
        message: &[u8],
    ) -> Self::SchnorrScalar;
    fn signature_from_scalar_and_nonce(
        scalar: Self::SchnorrScalar,
        nonce: Self::PublicNonce,
    ) -> Self::SchnorrSignature;
    fn verify_signature(
        public_key: &Self::PublicKey,
        message: &[u8],
        sig: &Self::SchnorrSignature,
    ) -> bool;
    fn sign(keypair: &Self::KeyPair, message: &[u8]) -> Self::SchnorrSignature;
}
