use core::fmt::Debug;

use alloc::vec::Vec;

use crate::OracleKeys;

#[cfg(not(feature = "postgres-types"))]
pub trait GroupObject:
    PartialEq
    + Clone
    + Debug
    + serde::Serialize
    + serde::de::DeserializeOwned
    + core::fmt::Display
    + Send
    + Sync
    + 'static
{
}

#[cfg(feature = "postgres-types")]
pub trait GroupObject:
    PartialEq
    + Clone
    + Debug
    + serde::Serialize
    + serde::de::DeserializeOwned
    + core::fmt::Display
    + Send
    + Sync
    + postgres_types::FromSqlOwned
    + postgres_types::ToSql
    + 'static
{
}

pub trait Group:
    Clone
    + Default
    + PartialEq
    + serde::de::DeserializeOwned
    + serde::Serialize
    + 'static
    + Send
    + Sync
    + Debug
{
    type AttestScalar: GroupObject;
    type PublicKey: GroupObject;
    type PublicNonce: GroupObject;
    type Signature: GroupObject;
    type AnticipatedAttestation;
    type NonceKeyPair: Into<Self::PublicNonce> + Clone + Debug;
    type KeyPair: Into<Self::PublicKey> + Clone;
    type EcdsaSignature: GroupObject;

    const KEY_MATERIAL_LEN: usize;

    fn name() -> &'static str;

    fn reveal_attest_scalar(
        signing_key: &Self::KeyPair,
        nonce_key: Self::NonceKeyPair,
        index: u32,
    ) -> Self::AttestScalar;

    fn verify_attest_scalar(
        attest_key: &Self::PublicKey,
        nonce_key: &Self::PublicNonce,
        index: u32,
        attest_scalar: &Self::AttestScalar,
    ) -> bool;

    fn verify_announcement_signature(
        public_key: &Self::PublicKey,
        message: &[u8],
        sig: &Self::Signature,
    ) -> bool;

    fn anticipate_attestations(
        public_key: &Self::PublicKey,
        public_nonce: &Self::PublicNonce,
        n_outcomes: u32,
    ) -> Vec<Self::AnticipatedAttestation>;

    fn sign_announcement(keypair: &Self::KeyPair, announcement: &[u8]) -> Self::Signature;
    fn keypair_from_secret_bytes(bytes: &[u8]) -> Self::KeyPair;
    fn nonce_keypair_from_secret_bytes(bytes: &[u8]) -> Self::NonceKeyPair;
    fn ecdsa_sign(keypair: &Self::KeyPair, message: &[u8]) -> Self::EcdsaSignature;
    fn ecdsa_verify(keypair: &Self::PublicKey, message: &[u8], sig: &Self::EcdsaSignature) -> bool;
    fn test_keypair() -> Self::KeyPair;
    fn test_nonce_keypair() -> Self::NonceKeyPair;
    fn test_oracle_keys() -> OracleKeys<Self>;
}

#[macro_export]
macro_rules! impl_deserialize_curve {
    ($curve:ident) => {
        impl<'de> serde::de::Deserialize<'de> for $curve {
            fn deserialize<D: serde::de::Deserializer<'de>>(
                deserializer: D,
            ) -> Result<$curve, D::Error> {
                use $crate::Group;
                let curve = String::deserialize(deserializer)?;
                if curve == $curve::name() {
                    Ok($curve::default())
                } else {
                    use serde::de::Error;
                    Err(D::Error::custom(format!(
                        "wrong curve, expected {} got {}",
                        $curve::name(),
                        curve
                    )))
                }
            }
        }

        impl serde::Serialize for $curve {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                use $crate::Group;
                serializer.serialize_str($curve::name())
            }
        }
    };
}
