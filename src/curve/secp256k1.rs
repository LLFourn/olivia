use crate::seed::Seed;

use diesel::sql_types;
use digest::{Input, VariableOutput};
pub use schnorr_fun::{
    fun::{self, marker::*, s, Scalar, XOnly, G},
    KeyPair, Schnorr, Signature,
};

pub struct Secp256k1;

#[derive(PartialEq, Clone, FromSqlRow, AsExpression)]
#[sql_type = "sql_types::Binary"]
pub struct PublicKey(XOnly<EvenY>);

crate::impl_display_debug_serialize_tosql! {
    fn to_bytes(pk: &PublicKey) -> &[u8;32] {
        pk.0.as_bytes()
    }
}

crate::impl_fromstr_deserailize_fromsql! {
    name => "secp256k1 xonly public key",
    fn from_bytes(bytes: [u8;32]) ->  Option<PublicKey> {
        XOnly::<EvenY>::from_bytes(bytes).map(PublicKey)
    }
}

#[derive(PartialEq, Clone, FromSqlRow, AsExpression)]
#[sql_type = "sql_types::Binary"]
pub struct PublicNonce(XOnly<SquareY>);

crate::impl_display_debug_serialize_tosql! {
    fn to_bytes(pn: &PublicNonce) -> &[u8;32] {
        pn.0.as_bytes()
    }
}

crate::impl_fromstr_deserailize_fromsql! {
    name => "secp256k1 xonly public nonce",
    fn from_bytes(bytes: [u8;32]) ->  Option<PublicNonce> {
        XOnly::<SquareY>::from_bytes(bytes).map(PublicNonce)
    }
}

#[derive(PartialEq, Clone, FromSqlRow, AsExpression)]
#[sql_type = "sql_types::Binary"]
pub struct SchnorrScalar(Scalar<Public, Zero>);

crate::impl_display_debug_serialize_tosql! {
    fn to_bytes(scalar: &SchnorrScalar) -> [u8;32] {
        scalar.0.to_bytes()
    }
}

crate::impl_fromstr_deserailize_fromsql! {
    name => "secp256k1 scalar",
    fn from_bytes(bytes: [u8;32]) ->  Option<SchnorrScalar> {
        Scalar::from_bytes(bytes).map(|s| SchnorrScalar(s.mark::<Public>()))
    }
}

lazy_static::lazy_static! {
    static ref SCHNORR: Schnorr = Schnorr::from_tag(b"oracle");
}

impl From<XOnly<EvenY>> for PublicKey {
    fn from(x: XOnly<EvenY>) -> Self {
        Self(x)
    }
}

impl From<KeyPair> for PublicKey {
    fn from(kp: KeyPair) -> Self {
        let (_, pk) = kp.into();
        PublicKey(pk)
    }
}

impl From<(Scalar, XOnly<SquareY>)> for PublicNonce {
    fn from(kp: (Scalar, XOnly<SquareY>)) -> Self {
        Self(kp.1)
    }
}

impl super::Curve for Secp256k1 {
    type KeyPair = KeyPair;
    type PublicKey = PublicKey;
    type PublicNonce = PublicNonce;
    type SchnorrScalar = SchnorrScalar;
    type NonceKeyPair = (Scalar, XOnly<SquareY>);
    type SchnorrSignature = Signature;

    fn derive_keypair(seed: &Seed) -> Self::KeyPair {
        let mut hash = seed.to_blake2b_32();
        hash.input(b"secp256k1");
        let x = Scalar::from_slice_mod_order(&hash.vec_result())
            .expect("hash output is 32-bytes long")
            .mark::<NonZero>()
            .expect("will not be zero");
        SCHNORR.new_keypair(x)
    }

    fn derive_nonce_keypair(seed: &Seed) -> Self::NonceKeyPair {
        let mut hash = seed.to_blake2b_32();
        hash.input(b"secp256k1");
        let mut r = Scalar::from_slice_mod_order(&hash.vec_result())
            .expect("hash output is 32-bytes long")
            .mark::<NonZero>()
            .expect("will not be zero");

        let R = XOnly::from_scalar_mul(&SCHNORR.G, &mut r);

        (r, R)
    }

    fn reveal_signature_s(
        signing_keypair: &Self::KeyPair,
        nonce_keypair: &Self::NonceKeyPair,
        message: &[u8],
    ) -> Self::SchnorrScalar {
        let (x, X) = signing_keypair.as_tuple();
        let (r, R) = nonce_keypair;
        let c = SCHNORR.challenge(R, X, message.mark::<Public>());
        let s = s!(r + c * x);
        SchnorrScalar(s.mark::<Public>())
    }

    fn signature_from_scalar_and_nonce(
        scalar: Self::SchnorrScalar,
        nonce: Self::PublicNonce,
    ) -> Self::SchnorrSignature {
        Signature {
            R: nonce.0,
            s: scalar.0,
        }
    }

    fn verify_signature(
        public_key: &Self::PublicKey,
        message: &[u8],
        sig: &Self::SchnorrSignature,
    ) -> bool {
        let public_key = public_key.0.clone().mark::<EvenY>();
        let message = message.mark::<Public>();
        let verification_key = public_key.to_point();
        SCHNORR.verify(&verification_key, message, sig)
    }

    fn sign(keypair: &Self::KeyPair, message: &[u8]) -> Self::SchnorrSignature {
        SCHNORR.sign(
            keypair,
            message.mark::<Public>(),
            fun::hash::Derivation::rng(&mut rand::thread_rng()),
        )
    }
}
