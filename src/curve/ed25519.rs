#![allow(non_snake_case)]
use crate::seed::Seed;
pub use curve25519_dalek::{
    edwards::{CompressedEdwardsY, EdwardsPoint},
    scalar::Scalar,
};
use diesel::sql_types;
use digest::generic_array::typenum::U64;
use sha2::{Digest, Sha512};

pub struct Ed25519;

#[derive(PartialEq, Clone, FromSqlRow, AsExpression)]
#[sql_type = "sql_types::Binary"]
pub struct PublicKey(EdwardsPoint);

crate::impl_display_debug_serialize! {
    fn to_bytes(pk: &PublicKey) -> [u8;32] {
        pk.0.compress().to_bytes()
    }
}

crate::impl_fromstr_deserailize! {
    name => "ed25519 compressed edwards Y coordinate",
    fn from_bytes(bytes: [u8;32]) -> Option<PublicKey> {
        CompressedEdwardsY(bytes).decompress().map(PublicKey)
    }
}

#[derive(PartialEq, Clone, FromSqlRow, AsExpression)]
#[sql_type = "sql_types::Binary"]
pub struct SchnorrScalar(Scalar);

crate::impl_display_debug_serialize! {
    fn to_bytes(scalar: &SchnorrScalar) -> &[u8;32] {
        scalar.0.as_bytes()
    }
}

#[derive(Debug, Clone)]
pub struct KeyPair {
    pub secret_key: Scalar,
    pub public_key: PublicKey,
}

impl KeyPair {
    pub fn from_scalar(scalar: Scalar) -> Self {
        let public_key = &scalar * &curve25519_dalek::constants::ED25519_BASEPOINT_TABLE;
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

impl super::Curve for Ed25519 {
    type KeyPair = KeyPair;
    type SchnorrScalar = SchnorrScalar;
    type PublicKey = PublicKey;
    type SchnorrSignature = ed25519_dalek::Signature;

    fn derive_keypair(seed: &Seed) -> Self::KeyPair {
        let mut hash = seed.to_blake2b();
        hash.input(b"ed25519");
        KeyPair::from_hash(hash)
    }

    fn derive_nonce_keypair(seed: &Seed) -> Self::KeyPair {
        Self::derive_keypair(seed)
    }

    fn reveal_signature_s(
        signing_keypair: &Self::KeyPair,
        nonce_keypair: &Self::KeyPair,
        message: &[u8],
    ) -> Self::SchnorrScalar {
        let (x, X) = signing_keypair.as_tuple();
        let (r, R) = nonce_keypair.as_tuple();
        let c = {
            let mut h = Sha512::default();
            h.input(R.0.compress().as_bytes());
            h.input(X.0.compress().as_bytes());
            h.input(&message);
            Scalar::from_hash(h)
        };

        let s = r + (&c * x);

        SchnorrScalar(s)
    }

    fn signature_from_scalar_and_nonce(
        scalar: Self::SchnorrScalar,
        nonce: Self::PublicKey,
    ) -> Self::SchnorrSignature {
        let mut bytes = [0u8; 64];
        bytes[..32].copy_from_slice(nonce.0.compress().as_bytes());
        bytes[32..].copy_from_slice(scalar.0.as_bytes());
        ed25519_dalek::Signature::from_bytes(&bytes[..]).expect("it's in the correct form")
    }

    fn verify_signature(
        public_key: &Self::PublicKey,
        message: &[u8],
        sig: &Self::SchnorrSignature,
    ) -> bool {
        let pk = ed25519_dalek::PublicKey::from_bytes(public_key.0.compress().as_bytes())
            .expect("will always be correct since it comes directly from a point");

        pk.verify_strict(message, sig).is_ok()
    }
}

mod diesel_impl {
    use super::*;
    use diesel::{
        backend::Backend,
        deserialize::{self, *},
        serialize::{self, *},
    };
    use std::io::Write;

    impl<DB: Backend> FromSql<sql_types::Binary, DB> for SchnorrScalar
    where
        Vec<u8>: FromSql<sql_types::Binary, DB>,
    {
        fn from_sql(bytes: Option<&DB::RawValue>) -> deserialize::Result<Self> {
            let bytes = <Vec<u8> as FromSql<sql_types::Binary, DB>>::from_sql(bytes)?;
            let mut scalar_bytes = [0u8; 32];
            scalar_bytes.copy_from_slice(&bytes[..]);
            Ok(Self(
                curve25519_dalek::scalar::Scalar::from_canonical_bytes(scalar_bytes).ok_or(
                    format!(
                        "Invalid curve25519 scalar from database: {}",
                        crate::util::to_hex(&scalar_bytes)
                    ),
                )?,
            ))
        }
    }

    impl<DB: Backend> FromSql<sql_types::Binary, DB> for PublicKey
    where
        Vec<u8>: FromSql<sql_types::Binary, DB>,
    {
        fn from_sql(bytes: Option<&DB::RawValue>) -> deserialize::Result<Self> {
            let bytes = <Vec<u8> as FromSql<sql_types::Binary, DB>>::from_sql(bytes)?;
            let mut compressed_point = [0u8; 32];
            compressed_point.copy_from_slice(&bytes[..]);
            Ok(Self(
                CompressedEdwardsY(compressed_point)
                    .decompress()
                    .ok_or("Invalid curve point".to_string())?,
            ))
        }
    }

    impl<DB: Backend> ToSql<sql_types::Binary, DB> for PublicKey {
        fn to_sql<W: Write>(&self, out: &mut Output<W, DB>) -> serialize::Result {
            ToSql::<sql_types::Binary, DB>::to_sql(self.0.compress().as_bytes().as_ref(), out)
        }
    }

    impl<DB: Backend> ToSql<sql_types::Binary, DB> for SchnorrScalar {
        fn to_sql<W: Write>(&self, out: &mut Output<W, DB>) -> serialize::Result {
            ToSql::<sql_types::Binary, DB>::to_sql(self.0.as_bytes().as_ref(), out)
        }
    }
}
