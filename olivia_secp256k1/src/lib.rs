#![allow(non_snake_case)]
pub use ecdsa_fun;
use olivia_core::{GroupObject, OracleKeys};
#[doc(hidden)]
pub use schnorr_fun::fun::hex;
pub use schnorr_fun::{self, fun, KeyPair};
use schnorr_fun::{
    fun::{g, marker::*, nonce::Deterministic, s, Point, Scalar, XOnly, G},
    Message, Schnorr,
};
pub use serde;
use sha2::{Digest, Sha256};
mod macros;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Secp256k1;

#[derive(PartialEq, Clone)]
pub struct PublicKey(XOnly);

crate::impl_display_debug_serialize_tosql! {
    fn to_bytes(pk: &PublicKey) -> &[u8;32] {
        pk.0.as_bytes()
    }
}

crate::impl_fromstr_deserialize_fromsql! {
    name => "secp256k1 xonly public key",
    fn from_bytes(bytes: [u8;32]) ->  Option<PublicKey> {
        XOnly::from_bytes(bytes).map(PublicKey)
    }
}

impl GroupObject for PublicKey {}

#[derive(PartialEq, Clone)]
pub struct PublicNonce(XOnly);
impl GroupObject for PublicNonce {}

crate::impl_display_debug_serialize_tosql! {
    fn to_bytes(pn: &PublicNonce) -> &[u8;32] {
        pn.0.as_bytes()
    }
}

crate::impl_fromstr_deserialize_fromsql! {
    name => "secp256k1 xonly public nonce",
    fn from_bytes(bytes: [u8;32]) ->  Option<PublicNonce> {
        XOnly::from_bytes(bytes).map(PublicNonce)
    }
}

#[derive(PartialEq, Clone)]
pub struct AttestScalar(Scalar<Public, Zero>);
impl GroupObject for AttestScalar {}

crate::impl_display_debug_serialize_tosql! {
    fn to_bytes(scalar: &AttestScalar) -> [u8;32] {
        scalar.0.to_bytes()
    }
}

crate::impl_fromstr_deserialize_fromsql! {
    name => "secp256k1 scalar",
    fn from_bytes(bytes: [u8;32]) ->  Option<AttestScalar> {
        Scalar::from_bytes(bytes).map(|s| AttestScalar(s.mark::<Public>()))
    }
}

#[derive(PartialEq, Clone)]
pub struct Signature(schnorr_fun::Signature);

crate::impl_display_debug_serialize_tosql! {
    fn to_bytes(sig: &Signature) -> [u8;64] {
        sig.0.to_bytes()
    }
}

crate::impl_fromstr_deserialize_fromsql! {
    name => "bip340 schnorr signature",
    fn from_bytes(bytes: [u8;64]) ->  Option<Signature> {
        schnorr_fun::Signature::from_bytes(bytes).map(Signature)
    }
}

impl GroupObject for Signature {}

#[derive(PartialEq, Clone)]
pub struct EcdsaSignature(ecdsa_fun::Signature);

crate::impl_display_debug_serialize_tosql! {
    fn to_bytes(sig: &EcdsaSignature) -> [u8;64] {
        sig.0.to_bytes()
    }
}

crate::impl_fromstr_deserialize_fromsql! {
    name => "ecdsa signature",
    fn from_bytes(bytes: [u8;64]) ->  Option<EcdsaSignature> {
        ecdsa_fun::Signature::from_bytes(bytes).map(EcdsaSignature)
    }
}

impl GroupObject for EcdsaSignature {}

lazy_static::lazy_static! {
    pub static ref SCHNORR: Schnorr<Sha256, Deterministic<Sha256>> = Schnorr::new(Deterministic::<Sha256>::default());
    pub static ref ECDSA: ecdsa_fun::ECDSA<Deterministic<Sha256>> = ecdsa_fun::ECDSA::<Deterministic<Sha256>>::default();
}

impl From<XOnly> for PublicKey {
    fn from(x: XOnly) -> Self {
        Self(x)
    }
}

impl From<PublicKey> for XOnly {
    fn from(x: PublicKey) -> Self {
        x.0
    }
}

impl From<XOnly> for PublicNonce {
    fn from(x: XOnly) -> Self {
        Self(x)
    }
}

impl From<PublicNonce> for XOnly {
    fn from(x: PublicNonce) -> Self {
        x.0
    }
}

impl From<AttestScalar> for Scalar<Public, Zero> {
    fn from(att_scalar: AttestScalar) -> Self {
        att_scalar.0
    }
}

impl From<KeyPair> for PublicKey {
    fn from(kp: KeyPair) -> Self {
        let (_, pk) = kp.into();
        PublicKey(pk)
    }
}

impl From<(Scalar, XOnly)> for PublicNonce {
    fn from(kp: (Scalar, XOnly)) -> Self {
        Self(kp.1)
    }
}

impl olivia_core::Group for Secp256k1 {
    type KeyPair = KeyPair;
    type PublicKey = PublicKey;
    type PublicNonce = PublicNonce;
    type NonceKeyPair = (Scalar, XOnly);
    type Signature = Signature;
    type AttestScalar = AttestScalar;
    type AnticipatedAttestation = Point<Jacobian, Public, Zero>;
    const KEY_MATERIAL_LEN: usize = 32;

    fn name() -> &'static str {
        "secp256k1"
    }

    fn verify_announcement_signature(
        public_key: &Self::PublicKey,
        message: &[u8],
        sig: &Self::Signature,
    ) -> bool {
        let public_key = public_key.0.clone();
        let verification_key = public_key.to_point();
        SCHNORR.verify(
            &verification_key,
            Message::<Public>::plain("DLC/announcement", &message[..]),
            &sig.0,
        )
    }

    fn reveal_attest_scalar(
        signing_key: &Self::KeyPair,
        nonce_key: Self::NonceKeyPair,
        index: u32,
    ) -> Self::AttestScalar {
        let r = nonce_key.0;
        let c = Scalar::from(index);
        let x = signing_key.secret_key();
        AttestScalar(s!((c + 1) * r + x).mark::<Public>())
    }

    fn anticipate_attestations(
        public_key: &Self::PublicKey,
        public_nonce: &Self::PublicNonce,
        n_outcomes: u32,
    ) -> Vec<Self::AnticipatedAttestation> {
        let X = public_key.0.to_point().mark::<(Jacobian, Zero)>();
        let R = public_nonce.0.to_point().mark::<Jacobian>();
        (0..n_outcomes)
            .scan(X, |C: &mut Point<Jacobian, Public, Zero>, _| {
                *C = g!({ *C } + R);
                Some(*C)
            })
            .collect()
    }

    fn sign_announcement(keypair: &Self::KeyPair, announcement: &[u8]) -> Self::Signature {
        Signature(SCHNORR.sign(
            keypair,
            Message::<Public>::plain("DLC/announcement", announcement),
        ))
    }

    fn verify_attest_scalar(
        public_key: &Self::PublicKey,
        public_nonce: &Self::PublicNonce,
        index: u32,
        attest_scalar: &Self::AttestScalar,
    ) -> bool {
        let s = &attest_scalar.0;
        let R = public_nonce.0.to_point();
        let X = public_key.0.to_point();
        let c = Scalar::from(index);
        g!(s * G) == g!((c + 1) * R + X)
    }

    fn test_keypair() -> Self::KeyPair {
        SCHNORR.new_keypair(
            Scalar::from_bytes_mod_order([42u8; 32])
                .mark::<NonZero>()
                .unwrap(),
        )
    }

    fn test_nonce_keypair() -> Self::NonceKeyPair {
        let mut r = Scalar::from_bytes_mod_order([84u8; 32])
            .mark::<NonZero>()
            .unwrap();
        let R = XOnly::from_scalar_mul(G, &mut r);
        (r, R)
    }

    fn test_oracle_keys() -> OracleKeys<Self> {
        OracleKeys {
            announcement: PublicKey(XOnly::from_bytes([13u8; 32]).unwrap()),
            ecdsa_v1: Some(PublicKey(XOnly::from_bytes([14u8; 32]).unwrap())),
            olivia_v1: Some(PublicKey(XOnly::from_bytes([16u8; 32]).unwrap())),
            group: Secp256k1,
        }
    }

    fn keypair_from_secret_bytes(bytes: &[u8]) -> Self::KeyPair {
        SCHNORR.new_keypair(
            Scalar::from_slice_mod_order(bytes)
                .expect("will be 32 bytes long")
                .mark::<NonZero>()
                .expect("will not be zero"),
        )
    }

    fn nonce_keypair_from_secret_bytes(bytes: &[u8]) -> Self::NonceKeyPair {
        let mut r = Scalar::from_slice_mod_order(bytes)
            .expect("hash output is 32-bytes long")
            .mark::<NonZero>()
            .expect("will not be zero");

        let R = XOnly::from_scalar_mul(&SCHNORR.G(), &mut r);
        (r, R)
    }

    type EcdsaSignature = EcdsaSignature;

    fn ecdsa_sign(keypair: &Self::KeyPair, message: &[u8]) -> Self::EcdsaSignature {
        let message_hash = {
            let mut message_hash = [0u8; 32];
            let hash = Sha256::default().chain(message);
            message_hash.copy_from_slice(hash.finalize().as_ref());
            message_hash
        };
        EcdsaSignature(ECDSA.sign(keypair.secret_key(), &message_hash))
    }

    fn ecdsa_verify(
        public_key: &Self::PublicKey,
        message: &[u8],
        sig: &Self::EcdsaSignature,
    ) -> bool {
        let message_hash = {
            let mut message_hash = [0u8; 32];
            let hash = Sha256::default().chain(message);
            message_hash.copy_from_slice(hash.finalize().as_ref());
            message_hash
        };
        ECDSA.verify(&public_key.0.to_point(), &message_hash, &sig.0)
    }
}

olivia_core::impl_deserialize_curve!(Secp256k1);

#[cfg(test)]
mod test {
    use super::*;
    use olivia_core::Group;
    #[test]
    fn anticipate_vs_attest() {
        let oracle_key = Secp256k1::test_keypair();
        let nonce_key = Secp256k1::test_nonce_keypair();
        let attestation_points = Secp256k1::anticipate_attestations(
            &oracle_key.clone().into(),
            &nonce_key.clone().into(),
            5,
        );
        let expected = (0..5)
            .map(|i| {
                g!(
                    { Secp256k1::reveal_attest_scalar(&oracle_key, nonce_key.clone(), i as u32).0 }
                        * G
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(attestation_points, expected);
    }

    #[test]
    fn test_oracle_keys() {
        let _ = Secp256k1::test_oracle_keys();
    }
}
