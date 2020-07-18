#![allow(non_snake_case)]
use crate::seed::Seed;
pub use curve25519_dalek::{
    constants::ED25519_BASEPOINT_TABLE,
    edwards::{CompressedEdwardsY, EdwardsPoint},
    scalar::Scalar,
};
use diesel::sql_types;
use digest::generic_array::typenum::U64;
use ed25519_dalek::ed25519::signature::Signature;
use rand::RngCore;
use sha2::{Digest, Sha512};

pub struct Ed25519;

#[derive(PartialEq, Clone, FromSqlRow, AsExpression)]
#[sql_type = "sql_types::Binary"]
pub struct PublicKey(EdwardsPoint);

crate::impl_display_debug_serialize_tosql! {
    fn to_bytes(pk: &PublicKey) -> [u8;32] {
        pk.0.compress().to_bytes()
    }
}

crate::impl_fromstr_deserailize_fromsql! {
    name => "ed25519 compressed edwards Y coordinate",
    fn from_bytes(bytes: [u8;32]) -> Option<PublicKey> {
        CompressedEdwardsY(bytes).decompress().map(PublicKey)
    }
}

#[derive(PartialEq, Clone, FromSqlRow, AsExpression)]
#[sql_type = "sql_types::Binary"]
pub struct SchnorrScalar(Scalar);

crate::impl_display_debug_serialize_tosql! {
    fn to_bytes(scalar: &SchnorrScalar) -> &[u8;32] {
        scalar.0.as_bytes()
    }
}

crate::impl_fromstr_deserailize_fromsql! {
    name => "ed25519 scalar",
    fn from_bytes(bytes: [u8;32]) ->  Option<SchnorrScalar> {
        Scalar::from_canonical_bytes(bytes).map(SchnorrScalar)
    }
}
#[derive(PartialEq, Clone, FromSqlRow, AsExpression)]
#[sql_type = "sql_types::Binary"]
pub struct SchnorrSignature(ed25519_dalek::Signature);

crate::impl_display_debug_serialize_tosql! {
    fn to_bytes(scalar: &SchnorrSignature) -> &[u8;64] {
        scalar.0.as_bytes()
    }
}

crate::impl_fromstr_deserailize_fromsql! {
    name => "ed25519 signature",
    fn from_bytes(bytes: [u8;64]) ->  Option<SchnorrSignature> {
        ed25519_dalek::Signature::from_bytes(&bytes[..]).map(SchnorrSignature).ok()
    }
}

#[derive(Debug, Clone)]
pub struct KeyPair {
    pub secret_key: Scalar,
    pub public_key: PublicKey,
}

impl KeyPair {
    pub fn from_scalar(scalar: Scalar) -> Self {
        let public_key = &scalar * &ED25519_BASEPOINT_TABLE;
        KeyPair {
            secret_key: scalar,
            public_key: PublicKey(public_key),
        }
    }

    pub fn from_hash<D: Digest<OutputSize = U64>>(hash: D) -> Self {
        Self::from_scalar(Scalar::from_hash(hash))
    }

    pub fn as_tuple(&self) -> (&Scalar, &PublicKey) {
        (&self.secret_key, &self.public_key)
    }
}

impl From<KeyPair> for PublicKey {
    fn from(keypair: KeyPair) -> Self {
        keypair.public_key
    }
}

lazy_static::lazy_static! {
    static ref HASH_1: sha2::Sha512 = sha2::Sha512::default().chain(&[0xFEu8])
        .chain(&[0xFFu8;31]);
}

impl super::Curve for Ed25519 {
    type KeyPair = KeyPair;
    type SchnorrScalar = SchnorrScalar;
    type PublicKey = PublicKey;
    type SchnorrSignature = SchnorrSignature;
    type PublicNonce = PublicKey;
    type NonceKeyPair = KeyPair;

    fn derive_keypair(seed: &Seed) -> Self::KeyPair {
        let mut hash = seed.to_blake2b();
        hash.input(b"ed25519");
        KeyPair::from_hash(hash)
    }

    fn derive_nonce_keypair(seed: &Seed) -> Self::NonceKeyPair {
        Self::derive_keypair(seed)
    }

    fn reveal_signature_s(
        signing_keypair: &Self::KeyPair,
        nonce_keypair: &Self::NonceKeyPair,
        message: &[u8],
    ) -> Self::SchnorrScalar {
        let (a, A) = signing_keypair.as_tuple();
        let (r, R) = nonce_keypair.as_tuple();
        let c = {
            let mut h = Sha512::default();
            h.input(R.0.compress().as_bytes());
            h.input(A.0.compress().as_bytes());
            h.input(&message);
            Scalar::from_hash(h)
        };

        let s = r + &c * a;

        SchnorrScalar(s)
    }

    fn signature_from_scalar_and_nonce(
        scalar: Self::SchnorrScalar,
        nonce: Self::PublicNonce,
    ) -> Self::SchnorrSignature {
        let mut bytes = [0u8; 64];
        bytes[..32].copy_from_slice(nonce.0.compress().as_bytes());
        bytes[32..].copy_from_slice(scalar.0.as_bytes());
        SchnorrSignature(
            ed25519_dalek::Signature::from_bytes(&bytes[..]).expect("it's in the correct form"),
        )
    }

    fn verify_signature(
        public_key: &Self::PublicKey,
        message: &[u8],
        sig: &Self::SchnorrSignature,
    ) -> bool {
        let pk = ed25519_dalek::PublicKey::from_bytes(public_key.0.compress().as_bytes())
            .expect("will always be correct since it comes directly from a point");

        pk.verify_strict(message, &sig.0).is_ok()
    }

    fn sign(keypair: &Self::KeyPair, message: &[u8]) -> Self::SchnorrSignature {
        let (a, A) = keypair.as_tuple();
        let mut Z = [0u8; 64];
        rand::thread_rng().fill_bytes(&mut Z[..]);

        let r = Scalar::from_hash(
            HASH_1
                .clone()
                .chain(a.as_bytes())
                .chain(message)
                .chain(&Z[..]),
        );
        let R = &r * &ED25519_BASEPOINT_TABLE;

        let c = {
            let mut h = Sha512::default();
            h.input(R.compress().as_bytes());
            h.input(A.0.compress().as_bytes());
            h.input(&message);
            Scalar::from_hash(h)
        };

        let s = r + &c * a;

        Self::signature_from_scalar_and_nonce(SchnorrScalar(s), PublicKey(R))
    }
}
