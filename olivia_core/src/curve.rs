pub trait Curve: Clone + Default + PartialEq + serde::Serialize + 'static + Send + Sync {
    type SchnorrScalar: PartialEq
        + Clone
        + core::fmt::Debug
        + serde::Serialize
        + serde::de::DeserializeOwned
        + core::fmt::Display
        + Send
        + Sync
        + 'static;

    type PublicKey: PartialEq
        + Clone
        + core::fmt::Debug
        + serde::Serialize
        + serde::de::DeserializeOwned
        + Send
        + Sync
        + 'static;

    type PublicNonce: PartialEq
        + Clone
        + core::fmt::Debug
        + serde::Serialize
        + serde::de::DeserializeOwned
        + core::fmt::Display
        + Send
        + Sync
        + 'static;

    type SchnorrSignature: PartialEq
        + Clone
        + core::fmt::Debug
        + serde::Serialize
        + serde::de::DeserializeOwned
        + Send
        + Sync
        + 'static;

    type KeyPair: Into<Self::PublicKey> + Clone;
    type NonceKeyPair: Into<Self::PublicNonce> + Clone;

    fn name() -> &'static str;

    fn reveal_signature_s(
        signing_key: &Self::KeyPair,
        nonce_key: Self::NonceKeyPair,
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

#[macro_export]
macro_rules! impl_deserialize_curve {
    ($curve:ident) => {
        impl<'de> serde::de::Deserialize<'de> for $curve {
            fn deserialize<D: serde::de::Deserializer<'de>>(
                deserializer: D,
            ) -> Result<$curve, D::Error> {
                use $crate::Curve;
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
            fn serialize<S: serde::Serializer>(&self,serializer: S) -> Result<S::Ok, S::Error> {
                use $crate::Curve;
                serializer.serialize_str($curve::name())
            }
        }
    };
}
